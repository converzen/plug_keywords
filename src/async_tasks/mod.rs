//! Singleton Tokio runtime management
//!
//! This module provides a global, thread-safe Tokio runtime that is
//! created once and reused for all async operations in the plugin.

use crate::{get_config, PluginConfig};
use anyhow::anyhow;

mod morsels;
// use directory::init_directory;
use crate::async_tasks::morsels::{init_failed_keywords, init_morsels};
use log::{debug, error, info};
pub use morsels::MorselEntry;
use std::sync::mpsc;
use std::time::Duration;

enum InitResult {
    Success,
    Error(String),
}

pub fn run_async_tasks() -> anyhow::Result<()> {
    info!("MCP plugin gmc-v2 starting async initialization");
    let (tx, rx) = mpsc::channel();
    let config = get_config().clone();
    debug!("starting async worker");
    std::thread::spawn::<_, Result<(), String>>(move || {
        debug!("starting async runtime");
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                let message = format!("failed to create runtime: {err}");
                tx.send(InitResult::Error(message))
                    .expect("failed to send message");
                return Ok(());
            }
        };

        debug!("initializing trigrams from database");
        match runtime.block_on(initialize_data()) {
            Ok(()) => tx
                .send(InitResult::Success)
                .expect("failed to send message"),
            Err(e) => {
                let message = format!("failed initialize data: {e}");
                tx.send(InitResult::Error(message))
                    .expect("failed to send message");
                return Ok(());
            }
        }

        // should run forever
        debug!(
            "starting update loop with update interval {}s",
            config.update_interval_secs.unwrap()
        );
        let res = runtime
            .block_on(update_loop(&config))
            .map_err(|e| e.to_string());
        info!("async thread is terminating with {res:?}");
        res
    });

    let res = rx
        .recv()
        .map_err(|err| anyhow!("Async thread receive error: {err}"))?;
    match res {
        InitResult::Success => Ok(()),
        InitResult::Error(message) => Err(anyhow!("{message}")),
    }
}

async fn initialize_data() -> anyhow::Result<()> {
    let config = get_config();
    init_morsels(config).await?;
    // tolerate this failure
    let _ = init_failed_keywords(config).await;
    Ok(())
}

async fn update_loop(config: &PluginConfig) -> anyhow::Result<()> {
    if config.update_interval_secs.unwrap() > 0 {
        loop {
            tokio::time::sleep(Duration::from_secs(
                config.update_interval_secs.unwrap() as u64
            ))
            .await;
            match initialize_data().await {
                Ok(()) => {
                    info!("update loop load data successfully");
                }
                Err(e) => {
                    error!("update loop failed to load data: {e}");
                }
            }
        }
    }

    Ok(())
}
