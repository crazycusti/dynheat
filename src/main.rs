mod config;
mod controller;
mod outputs;
mod sensors;
mod web;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::{net::TcpListener, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use tracing_subscriber::prelude::*;

use crate::config::HeatingConfig;
use crate::controller::HeatingController;
use crate::web::{AppContext, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dynheat=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = HeatingConfig::default();
    let controller = HeatingController::from_config(&config).await?;

    let shared_config = Arc::new(RwLock::new(config));
    let shared_controller = Arc::new(RwLock::new(controller));

    let scheduler_config = Arc::clone(&shared_config);
    let scheduler_controller = Arc::clone(&shared_controller);

    tokio::spawn(async move {
        loop {
            let config_snapshot = scheduler_config.read().await.clone();
            {
                let controller_guard = scheduler_controller.read().await;
                if let Err(err) = controller_guard.evaluate(&config_snapshot).await {
                    error!("scheduled evaluation failed: {err}");
                }
            }
            tokio::time::sleep(config_snapshot.interval_duration()).await;
        }
    });

    let ctx = AppContext {
        config: Arc::clone(&shared_config),
        controller: Arc::clone(&shared_controller),
    };

    let app = router(ctx)
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = ([0, 0, 0, 0], 8080).into();
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "DynHeat Server gestartet");

    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
