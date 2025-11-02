use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::outputs::OutputSettings;
use crate::sensors::SensorSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HeatingConfig {
    pub comfort_temperature: f32,
    pub setback_temperature: f32,
    pub design_outdoor_temperature: f32,
    pub heating_curve_slope: f32,
    pub heating_curve_base: f32,
    pub lux_reference: f32,
    pub lux_influence: f32,
    pub control_runs_per_day: u32,
    pub output_min: f32,
    pub output_max: f32,
    pub sensors: Vec<SensorSettings>,
    pub outputs: Vec<OutputSettings>,
}

impl Default for HeatingConfig {
    fn default() -> Self {
        Self {
            comfort_temperature: 21.0,
            setback_temperature: 18.5,
            design_outdoor_temperature: -10.0,
            heating_curve_slope: 1.6,
            heating_curve_base: 30.0,
            lux_reference: 800.0,
            lux_influence: 3.0,
            control_runs_per_day: 24,
            output_min: 0.0,
            output_max: 1.0,
            sensors: vec![SensorSettings::JsonFile {
                path: "data/sample_environment.json".into(),
                friendly_name: "Sample JSON".into(),
            }],
            outputs: vec![OutputSettings::Log {
                name: "Console".into(),
            }],
        }
    }
}

impl HeatingConfig {
    pub fn interval_duration(&self) -> Duration {
        let runs = self.control_runs_per_day.max(1);
        Duration::from_secs(24 * 60 * 60 / runs as u64)
    }

    pub fn update(&mut self, update: HeatingConfigUpdate) {
        if let Some(value) = update.comfort_temperature {
            self.comfort_temperature = value;
        }
        if let Some(value) = update.setback_temperature {
            self.setback_temperature = value;
        }
        if let Some(value) = update.design_outdoor_temperature {
            self.design_outdoor_temperature = value;
        }
        if let Some(value) = update.heating_curve_slope {
            self.heating_curve_slope = value;
        }
        if let Some(value) = update.heating_curve_base {
            self.heating_curve_base = value;
        }
        if let Some(value) = update.lux_reference {
            self.lux_reference = value;
        }
        if let Some(value) = update.lux_influence {
            self.lux_influence = value;
        }
        if let Some(value) = update.control_runs_per_day {
            self.control_runs_per_day = value.max(1);
        }
        if let Some(value) = update.output_min {
            self.output_min = value;
        }
        if let Some(value) = update.output_max {
            self.output_max = value;
        }
        if let Some(sensors) = update.sensors {
            self.sensors = sensors;
        }
        if let Some(outputs) = update.outputs {
            self.outputs = outputs;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HeatingConfigUpdate {
    pub comfort_temperature: Option<f32>,
    pub setback_temperature: Option<f32>,
    pub design_outdoor_temperature: Option<f32>,
    pub heating_curve_slope: Option<f32>,
    pub heating_curve_base: Option<f32>,
    pub lux_reference: Option<f32>,
    pub lux_influence: Option<f32>,
    pub control_runs_per_day: Option<u32>,
    pub output_min: Option<f32>,
    pub output_max: Option<f32>,
    pub sensors: Option<Vec<SensorSettings>>,
    pub outputs: Option<Vec<OutputSettings>>,
}
