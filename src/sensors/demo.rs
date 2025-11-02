use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{Sensor, SensorError, SensorSnapshot};

pub struct DemoWeatherSensor {
    name: String,
}

impl DemoWeatherSensor {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

#[async_trait]
impl Sensor for DemoWeatherSensor {
    async fn read(&self) -> Result<SensorSnapshot, SensorError> {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut rng = StdRng::seed_from_u64(seed);
        let outside = rng.gen_range(-5.0..15.0);
        let lux = rng.gen_range(100.0..1200.0);
        let forecast = outside + rng.gen_range(-3.0..3.0);
        let humidity = rng.gen_range(0.3..0.9) * 100.0;

        Ok(SensorSnapshot {
            outside_temperature: Some(outside),
            indoor_temperature: Some(rng.gen_range(19.0..23.0)),
            lux: Some(lux),
            forecast_temperature: Some(forecast),
            humidity: Some(humidity),
        })
    }

    fn name(&self) -> &str {
        &self.name
    }
}
