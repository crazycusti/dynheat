use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use thiserror::Error;

pub mod demo;
pub mod json_file;
pub mod mqtt;

pub use demo::DemoWeatherSensor;
pub use json_file::JsonFileSensor;
pub use mqtt::MqttSensor;

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SensorSnapshot {
    pub outside_temperature: Option<f32>,
    pub indoor_temperature: Option<f32>,
    pub lux: Option<f32>,
    pub forecast_temperature: Option<f32>,
    pub humidity: Option<f32>,
}

impl SensorSnapshot {
    pub fn merge(&mut self, other: SensorSnapshot) {
        if self.outside_temperature.is_none() {
            self.outside_temperature = other.outside_temperature;
        }
        if self.indoor_temperature.is_none() {
            self.indoor_temperature = other.indoor_temperature;
        }
        if self.lux.is_none() {
            self.lux = other.lux;
        }
        if self.forecast_temperature.is_none() {
            self.forecast_temperature = other.forecast_temperature;
        }
        if self.humidity.is_none() {
            self.humidity = other.humidity;
        }
    }
}

#[derive(Debug, Error)]
pub enum SensorError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("mqtt error: {0}")]
    Mqtt(String),
    #[error("no data available from sensor {0}")]
    Empty(String),
}

#[async_trait]
pub trait Sensor: Send + Sync {
    async fn read(&self) -> Result<SensorSnapshot, SensorError>;
    fn name(&self) -> &str;
}

pub type DynSensor = Arc<dyn Sensor>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SensorSettings {
    JsonFile {
        path: String,
        friendly_name: String,
    },
    Mqtt {
        friendly_name: String,
        broker: String,
        topic: String,
        payload_key: Option<String>,
    },
    DemoWeather {
        friendly_name: String,
    },
}

impl SensorSettings {
    pub async fn build(&self) -> Result<DynSensor, SensorError> {
        match self {
            SensorSettings::JsonFile {
                path,
                friendly_name,
            } => Ok(Arc::new(JsonFileSensor::new(
                friendly_name.clone(),
                path.clone(),
            ))),
            SensorSettings::Mqtt {
                friendly_name,
                broker,
                topic,
                payload_key,
            } => Ok(MqttSensor::connect(
                friendly_name.clone(),
                broker.clone(),
                topic.clone(),
                payload_key.clone(),
            )
            .await?),
            SensorSettings::DemoWeather { friendly_name } => {
                Ok(Arc::new(DemoWeatherSensor::new(friendly_name.clone())))
            }
        }
    }
}
