use std::sync::Arc;

use axum::extract::State;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::error;

use crate::config::{HeatingConfig, HeatingConfigUpdate};
use crate::controller::{ControllerStatus, HeatingController};
use crate::outputs::OutputCommand;

#[derive(Clone)]
pub struct AppContext {
    pub config: Arc<RwLock<HeatingConfig>>,
    pub controller: Arc<RwLock<HeatingController>>,
}

pub fn router(ctx: AppContext) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/status", get(get_status))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/trigger", post(trigger_now))
        .route("/api/sensors", get(list_sensors))
        .route("/api/outputs", get(list_outputs))
        .with_state(ctx)
}

async fn index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("index.html"))
}

async fn get_status(State(ctx): State<AppContext>) -> Json<ControllerStatus> {
    let controller = ctx.controller.read().await;
    Json(controller.status().await)
}

async fn get_config(State(ctx): State<AppContext>) -> Json<HeatingConfig> {
    let config = ctx.config.read().await;
    Json(config.clone())
}

async fn update_config(
    State(ctx): State<AppContext>,
    Json(update): Json<HeatingConfigUpdate>,
) -> Result<Json<HeatingConfig>, (axum::http::StatusCode, String)> {
    let mut config = ctx.config.write().await;
    config.update(update);
    let new_config = config.clone();
    drop(config);

    let mut controller = ctx.controller.write().await;
    if let Err(err) = controller.reload(&new_config).await {
        error!("controller reload failed: {err}");
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("controller reload failed: {err}"),
        ));
    }

    Ok(Json(new_config))
}

async fn trigger_now(
    State(ctx): State<AppContext>,
) -> Result<Json<OutputCommand>, (axum::http::StatusCode, String)> {
    let config = ctx.config.read().await.clone();
    let controller = ctx.controller.read().await;
    controller.evaluate(&config).await.map(Json).map_err(|err| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
    })
}

#[derive(Serialize)]
struct SensorsResponse {
    sensors: Vec<String>,
}

async fn list_sensors(State(ctx): State<AppContext>) -> Json<SensorsResponse> {
    let config = ctx.config.read().await;
    let names = config
        .sensors
        .iter()
        .map(|sensor| match sensor {
            crate::sensors::SensorSettings::JsonFile { friendly_name, .. } => friendly_name.clone(),
            crate::sensors::SensorSettings::Mqtt { friendly_name, .. } => friendly_name.clone(),
            crate::sensors::SensorSettings::DemoWeather { friendly_name } => friendly_name.clone(),
        })
        .collect();
    Json(SensorsResponse { sensors: names })
}

#[derive(Serialize)]
struct OutputsResponse {
    outputs: Vec<String>,
}

async fn list_outputs(State(ctx): State<AppContext>) -> Json<OutputsResponse> {
    let config = ctx.config.read().await;
    let names = config
        .outputs
        .iter()
        .map(|output| match output {
            crate::outputs::OutputSettings::ShellyRelay { name, .. } => name.clone(),
            crate::outputs::OutputSettings::Mqtt { name, .. } => name.clone(),
            crate::outputs::OutputSettings::Log { name } => name.clone(),
        })
        .collect();
    Json(OutputsResponse { outputs: names })
}
