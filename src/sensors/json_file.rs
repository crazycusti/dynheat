use async_trait::async_trait;
use serde::Deserialize;
use tokio::fs;

use super::{Sensor, SensorError, SensorSnapshot};

#[derive(Debug, Deserialize)]
struct JsonSensorPayload {
    outside_temperature: Option<f32>,
    indoor_temperature: Option<f32>,
    lux: Option<f32>,
    forecast_temperature: Option<f32>,
    humidity: Option<f32>,
    #[serde(rename = "weather_forecast")]
    weather_forecast: Option<ForecastBlock>,
}

#[derive(Debug, Deserialize)]
struct ForecastBlock {
    temperature: Option<f32>,
}

pub struct JsonFileSensor {
    name: String,
    path: String,
}

impl JsonFileSensor {
    pub fn new(name: String, path: String) -> Self {
        Self { name, path }
    }
}

#[async_trait]
impl Sensor for JsonFileSensor {
    async fn read(&self) -> Result<SensorSnapshot, SensorError> {
        let content = fs::read_to_string(&self.path).await?;
        let payload: JsonSensorPayload = serde_json::from_str(&content)?;
        let snapshot = SensorSnapshot {
            outside_temperature: payload.outside_temperature,
            indoor_temperature: payload.indoor_temperature,
            lux: payload.lux,
            forecast_temperature: payload
                .forecast_temperature
                .or_else(|| payload.weather_forecast.and_then(|w| w.temperature)),
            humidity: payload.humidity,
        };

        if snapshot.outside_temperature.is_none()
            && snapshot.indoor_temperature.is_none()
            && snapshot.lux.is_none()
            && snapshot.forecast_temperature.is_none()
        {
            Err(SensorError::Empty(self.name.clone()))
        } else {
            Ok(snapshot)
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}
