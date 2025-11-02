use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use serde_json::{self, json};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{error, info};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputCommand {
    pub flow_temperature: f32,
    pub power_level: f32,
    pub comfort_setpoint: f32,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[async_trait]
pub trait OutputDevice: Send + Sync {
    async fn apply(&self, command: &OutputCommand) -> Result<(), OutputError>;
    fn name(&self) -> &str;
}

pub type DynOutput = Arc<dyn OutputDevice>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputSettings {
    ShellyRelay {
        name: String,
        base_url: String,
    },
    Mqtt {
        name: String,
        broker: String,
        topic: String,
    },
    Log {
        name: String,
    },
}

impl OutputSettings {
    pub async fn build(&self) -> Result<DynOutput, OutputError> {
        match self {
            OutputSettings::ShellyRelay { name, base_url } => {
                Ok(Arc::new(ShellyRelay::new(name.clone(), base_url.clone())?))
            }
            OutputSettings::Mqtt {
                name,
                broker,
                topic,
            } => Ok(MqttPublisher::connect(name.clone(), broker.clone(), topic.clone()).await?),
            OutputSettings::Log { name } => Ok(Arc::new(LogOutput::new(name.clone()))),
        }
    }
}

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("mqtt error: {0}")]
    Mqtt(String),
}

struct ShellyRelay {
    name: String,
    client: reqwest::Client,
    base_url: String,
}

impl ShellyRelay {
    fn new(name: String, base_url: String) -> Result<Self, OutputError> {
        Ok(Self {
            name,
            client: reqwest::Client::builder()
                .user_agent("dynheat/0.1")
                .build()?,
            base_url,
        })
    }
}

#[async_trait]
impl OutputDevice for ShellyRelay {
    async fn apply(&self, command: &OutputCommand) -> Result<(), OutputError> {
        let state = if command.power_level >= 0.5 {
            "on"
        } else {
            "off"
        };
        let brightness = (command.power_level.clamp(0.0, 1.0) * 100.0).round();
        let url = format!("{}/relay/0", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(&url)
            .json(&json!({
                "turn": state,
                "brightness": brightness,
                "temp": command.flow_temperature,
            }))
            .send()
            .await?;

        if response.status().is_success() {
            info!(output = %self.name, state, brightness, "Shelly relay updated");
        } else {
            error!(
                output = %self.name,
                status = ?response.status(),
                "Shelly relay request failed"
            );
        }

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

struct LogOutput {
    name: String,
}

impl LogOutput {
    fn new(name: String) -> Self {
        Self { name }
    }
}

#[async_trait]
impl OutputDevice for LogOutput {
    async fn apply(&self, command: &OutputCommand) -> Result<(), OutputError> {
        info!(
            output = %self.name,
            flow = command.flow_temperature,
            power = command.power_level,
            reason = %command.reason,
            "Log output dispatched"
        );
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

struct MqttPublisher {
    name: String,
    client: AsyncClient,
    topic: String,
    loop_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl MqttPublisher {
    async fn connect(
        name: String,
        broker: String,
        topic: String,
    ) -> Result<Arc<Self>, OutputError> {
        let url = Url::parse(&broker).map_err(|e| OutputError::Mqtt(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| OutputError::Mqtt("missing host in broker url".into()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| OutputError::Mqtt("missing port in broker url".into()))?;

        let mut options = MqttOptions::new(name.clone(), host, port);
        if !url.username().is_empty() {
            let password = url.password().unwrap_or_default().to_string();
            options.set_credentials(url.username(), password);
        }

        let (client, mut eventloop) = AsyncClient::new(options, 10);
        let log_name = name.clone();
        let join = tokio::spawn(async move {
            loop {
                if let Err(err) = eventloop.poll().await {
                    error!(output = %log_name, "MQTT loop error: {err}");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        });

        Ok(Arc::new(Self {
            name,
            client,
            topic,
            loop_handle: Mutex::new(Some(join)),
        }))
    }
}

#[async_trait]
impl OutputDevice for MqttPublisher {
    async fn apply(&self, command: &OutputCommand) -> Result<(), OutputError> {
        let payload = serde_json::to_vec(command).map_err(|e| OutputError::Mqtt(e.to_string()))?;
        self.client
            .publish(&self.topic, QoS::AtLeastOnce, false, payload)
            .await
            .map_err(|e| OutputError::Mqtt(e.to_string()))?;
        info!(output = %self.name, topic = %self.topic, "MQTT command published");
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for MqttPublisher {
    fn drop(&mut self) {
        if let Some(handle) = self.loop_handle.blocking_lock().take() {
            handle.abort();
        }
    }
}
