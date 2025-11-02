use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::warn;

use crate::config::HeatingConfig;
use crate::outputs::{DynOutput, OutputCommand, OutputSettings};
use crate::sensors::{DynSensor, SensorSettings, SensorSnapshot};

#[derive(Debug, Error)]
pub enum ControllerError {
    #[error("sensor error: {0}")]
    Sensor(String),
    #[error("output error: {0}")]
    Output(String),
}

#[derive(Clone, Default, Serialize)]
pub struct ControllerStatus {
    pub snapshot: Option<SensorSnapshot>,
    pub command: Option<OutputCommand>,
    pub errors: Vec<String>,
}

pub struct HeatingController {
    sensors: Vec<DynSensor>,
    outputs: Vec<DynOutput>,
    status: Arc<RwLock<ControllerStatus>>,
}

impl HeatingController {
    pub async fn from_config(config: &HeatingConfig) -> Result<Self, ControllerError> {
        let sensors = build_sensors(&config.sensors).await?;
        let outputs = build_outputs(&config.outputs).await?;
        Ok(Self {
            sensors,
            outputs,
            status: Arc::new(RwLock::new(ControllerStatus::default())),
        })
    }

    pub async fn reload(&mut self, config: &HeatingConfig) -> Result<(), ControllerError> {
        self.sensors = build_sensors(&config.sensors).await?;
        self.outputs = build_outputs(&config.outputs).await?;
        Ok(())
    }

    pub async fn evaluate(&self, config: &HeatingConfig) -> Result<OutputCommand, ControllerError> {
        let mut snapshot = SensorSnapshot::default();
        let mut errors = Vec::new();

        for sensor in &self.sensors {
            match sensor.read().await {
                Ok(data) => {
                    snapshot.merge(data);
                }
                Err(err) => {
                    errors.push(format!("{}: {err}", sensor.name()));
                }
            }
        }

        if snapshot.outside_temperature.is_none()
            && snapshot.indoor_temperature.is_none()
            && snapshot.forecast_temperature.is_none()
        {
            errors.push("No relevant sensor data available".into());
        }

        let command = self.compute_command(config, &snapshot);
        let mut status_guard = self.status.write().await;
        status_guard.snapshot = Some(snapshot.clone());
        status_guard.command = Some(command.clone());
        status_guard.errors = errors.clone();

        drop(status_guard);

        for output in &self.outputs {
            if let Err(err) = output.apply(&command).await {
                let message = format!("{}: {err}", output.name());
                warn!(output = %output.name(), "output failed: {err}");
                errors.push(message);
            }
        }

        if !errors.is_empty() {
            let mut status_guard = self.status.write().await;
            status_guard.errors = errors;
        }

        Ok(command)
    }

    pub async fn status(&self) -> ControllerStatus {
        self.status.read().await.clone()
    }

    fn compute_command(&self, config: &HeatingConfig, snapshot: &SensorSnapshot) -> OutputCommand {
        let outside = snapshot
            .outside_temperature
            .unwrap_or(config.design_outdoor_temperature);
        let forecast = snapshot
            .forecast_temperature
            .unwrap_or(snapshot.outside_temperature.unwrap_or(outside));
        let lux = snapshot.lux.unwrap_or(config.lux_reference);
        let indoor = snapshot
            .indoor_temperature
            .unwrap_or(config.comfort_temperature);

        let lux_ratio = (config.lux_reference - lux) / config.lux_reference.max(1.0);
        let lux_adjustment = (lux_ratio * config.lux_influence).clamp(-5.0, 5.0);

        let blended_outside = 0.6 * outside + 0.4 * forecast - lux_adjustment;
        let outside_factor = ((blended_outside - config.design_outdoor_temperature)
            / (config.comfort_temperature - config.design_outdoor_temperature))
            .clamp(0.0, 1.0);
        let target_room = config.setback_temperature
            + (config.comfort_temperature - config.setback_temperature) * (1.0 - outside_factor);

        let flow_temperature = (config.heating_curve_base
            + config.heating_curve_slope * (target_room - blended_outside))
            .max(target_room + 2.0);

        let temperature_error = target_room - indoor;
        let proportional = (temperature_error / 5.0).clamp(-1.0, 1.0);
        let integrator = ((target_room - config.setback_temperature) / 10.0).clamp(0.0, 1.0);
        let mut power_level = proportional + integrator;
        if outside < 0.0 {
            power_level += 0.1;
        }
        if forecast < outside {
            power_level += 0.05;
        }
        power_level = power_level.clamp(config.output_min, config.output_max);

        let reason = format!(
            "outside={outside:.1}°C forecast={forecast:.1}°C lux={lux:.0} target={target_room:.1}°C",
        );

        OutputCommand {
            flow_temperature,
            power_level,
            comfort_setpoint: target_room,
            reason,
            timestamp: Utc::now(),
        }
    }
}

async fn build_sensors(settings: &[SensorSettings]) -> Result<Vec<DynSensor>, ControllerError> {
    let mut sensors = Vec::new();
    for setting in settings {
        match setting.build().await {
            Ok(sensor) => sensors.push(sensor),
            Err(err) => {
                return Err(ControllerError::Sensor(format!("{setting:?}: {err}")));
            }
        }
    }
    if sensors.is_empty() {
        return Err(ControllerError::Sensor("no sensors configured".into()));
    }
    Ok(sensors)
}

async fn build_outputs(settings: &[OutputSettings]) -> Result<Vec<DynOutput>, ControllerError> {
    let mut outputs = Vec::new();
    for setting in settings {
        match setting.build().await {
            Ok(output) => outputs.push(output),
            Err(err) => {
                return Err(ControllerError::Output(format!("{setting:?}: {err}")));
            }
        }
    }
    if outputs.is_empty() {
        warn!("no outputs configured, results will not be applied");
    }
    Ok(outputs)
}
