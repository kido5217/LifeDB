//! Service entry point: config -> telemetry -> pool -> migrate -> serve
//! with graceful shutdown (design D7/D10, task 3.4).

use std::sync::Arc;

use anyhow::Context;
use lifedb::{app, config::Config, db, state::AppState, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env().context("reading environment configuration")?;
    telemetry::init(&config)?;

    let pool = db::connect(&config.database_url)
        .await
        .context("connecting to the database")?;
    db::MIGRATIONS
        .run(&pool)
        .await
        .context("applying migrations")?;

    let listen_addr = config.listen_addr.clone();
    let state = AppState::new(pool, Arc::new(config));
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("binding {listen_addr}"))?;
    tracing::info!(addr = %listen_addr, "listening");

    axum::serve(listener, app::build(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shutdown complete");
    Ok(())
}

/// Resolves on the first SIGINT or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
