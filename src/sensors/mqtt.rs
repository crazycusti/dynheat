use std::sync::Arc;

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use tokio::sync::RwLock;
use tracing::{error, info};
use url::Url;

use super::{Sensor, SensorError, SensorSnapshot};

pub struct MqttSensor {
    name: String,
    #[allow(dead_code)]
    client: AsyncClient,
    last_value: Arc<RwLock<Option<SensorSnapshot>>>,
}

impl MqttSensor {
    pub async fn connect(
        name: String,
        broker: String,
        topic: String,
        payload_key: Option<String>,
    ) -> Result<Arc<Self>, SensorError> {
        let url = Url::parse(&broker).map_err(|e| SensorError::Mqtt(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| SensorError::Mqtt("missing host in broker url".into()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| SensorError::Mqtt("missing port in broker url".into()))?;

        let mut options = MqttOptions::new(name.clone(), host, port);
        if !url.username().is_empty() {
            let password = url.password().unwrap_or_default().to_string();
            options.set_credentials(url.username(), password);
        }

        let (client, mut eventloop) = AsyncClient::new(options, 10);
        client
            .subscribe(topic.clone(), QoS::AtMostOnce)
            .await
            .map_err(|e| SensorError::Mqtt(e.to_string()))?;

        let last_value = Arc::new(RwLock::new(None));
        let loop_value = last_value.clone();
        let loop_topic = topic.clone();
        let loop_key = payload_key.clone();
        let loop_name = name.clone();

        tokio::spawn(async move {
            if let Err(err) = pump_loop(
                &loop_name,
                loop_topic,
                loop_key,
                &mut eventloop,
                loop_value.clone(),
            )
            .await
            {
                error!(sensor = %loop_name, "MQTT loop stopped: {err}");
            }
        });

        Ok(Arc::new(Self {
            name,
            client,
            last_value,
        }))
    }
}

#[async_trait]
impl Sensor for MqttSensor {
    async fn read(&self) -> Result<SensorSnapshot, SensorError> {
        let guard = self.last_value.read().await;
        guard
            .clone()
            .ok_or_else(|| SensorError::Empty(self.name.clone()))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

async fn pump_loop(
    sensor_name: &str,
    topic: String,
    payload_key: Option<String>,
    eventloop: &mut EventLoop,
    store: Arc<RwLock<Option<SensorSnapshot>>>,
) -> Result<(), SensorError> {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publication))) => {
                let payload = match std::str::from_utf8(&publication.payload) {
                    Ok(text) => text,
                    Err(err) => {
                        error!(sensor = sensor_name, "invalid utf8 payload: {err}");
                        continue;
                    }
                };

                if let Some(snapshot) = parse_payload(payload, payload_key.as_deref()) {
                    let mut guard = store.write().await;
                    *guard = Some(snapshot);
                    info!(sensor = sensor_name, topic = %topic, "MQTT update processed");
                }
            }
            Ok(_) => {}
            Err(err) => {
                error!(sensor = sensor_name, "MQTT error: {err}");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

fn parse_payload(payload: &str, key: Option<&str>) -> Option<SensorSnapshot> {
    if let Some(key) = key {
        let json: serde_json::Value = serde_json::from_str(payload).ok()?;
        let value = json.get(key)?;
        if let Some(number) = value.as_f64() {
            return Some(build_single_value_snapshot(key, number as f32));
        }
        if value.is_object() {
            return snapshot_from_object(value.as_object()?);
        }
        return None;
    }

    if let Ok(number) = payload.trim().parse::<f32>() {
        let mut snapshot = SensorSnapshot::default();
        snapshot.outside_temperature = Some(number);
        return Some(snapshot);
    }

    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    if json.is_object() {
        return snapshot_from_object(json.as_object()?);
    }

    None
}

fn snapshot_from_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<SensorSnapshot> {
    let mut snapshot = SensorSnapshot::default();
    for (key, value) in map {
        if let Some(number) = value.as_f64() {
            assign_key(&mut snapshot, key, number as f32);
        }
    }
    if snapshot.outside_temperature.is_some()
        || snapshot.indoor_temperature.is_some()
        || snapshot.lux.is_some()
        || snapshot.forecast_temperature.is_some()
    {
        Some(snapshot)
    } else {
        None
    }
}

fn build_single_value_snapshot(key: &str, value: f32) -> SensorSnapshot {
    let mut snapshot = SensorSnapshot::default();
    assign_key(&mut snapshot, key, value);
    snapshot
}

fn assign_key(snapshot: &mut SensorSnapshot, key: &str, value: f32) {
    match key {
        "outside_temperature" | "outside" | "outdoor" => {
            snapshot.outside_temperature = Some(value);
        }
        "indoor_temperature" | "indoor" | "room" => {
            snapshot.indoor_temperature = Some(value);
        }
        "lux" | "illuminance" => {
            snapshot.lux = Some(value);
        }
        "forecast_temperature" | "forecast" => {
            snapshot.forecast_temperature = Some(value);
        }
        "humidity" => {
            snapshot.humidity = Some(value);
        }
        _ => {
            snapshot.forecast_temperature = Some(value);
        }
    }
}
