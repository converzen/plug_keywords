//! Singleton Tokio runtime management
//!
//! This module provides a global, thread-safe Tokio runtime that is
//! created once and reused for all async operations in the plugin.

use crate::{PluginConfig, get_config};
use anyhow::anyhow;
use std::path::{Path, PathBuf};

mod trigrams;
pub use trigrams::*;
mod morsels;
use crate::async_tasks::morsels::{init_failed_keywords, init_morsels};
use log::{debug, error, info};
pub use morsels::{MorselEntry, MorselResult};
use std::sync::mpsc;
use std::time::Duration;

mod directory;
pub use directory::*;

enum InitResult {
    Success,
    Error(String),
}

pub fn run_async_tasks() -> anyhow::Result<()> {
    info!("ConverZen MCP plugin keywords starting async initialization");
    let (tx, rx) = mpsc::channel();
    let config = get_config().clone();
    debug!("starting async worker");
    std::thread::spawn::<_, Result<(), String>>(move || {
        debug!("starting async runtime");
        let runtime = match tokio::runtime::Builder::new_multi_thread()
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
            config.update_interval_secs
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
    let results = tokio::join!(
        init_morsels(config),
        init_directory(config),
        init_failed_keywords(config)
    );
    // tolerate this failure
    let mut func_count = 0;
    func_count += match results.0 {
        Ok(success) => {
            if success {
                1
            } else {
                0
            }
        }
        Err(e) => return Err(anyhow!("Failed to initialize morsels: {e}")),
    };

    func_count += match results.1 {
        Ok(success) => {
            if success {
                1
            } else {
                0
            }
        }
        Err(e) => return Err(anyhow!("Failed to initialize directory: {e}")),
    };

    if func_count > 0 {
        Ok(())
    } else {
        Err(anyhow!("Failed to initialize any async functions"))
    }
}

async fn update_loop(config: &PluginConfig) -> anyhow::Result<()> {
    if config.update_interval_secs > 0 {
        loop {
            tokio::time::sleep(Duration::from_secs(config.update_interval_secs as u64)).await;
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

fn validate_path<T: AsRef<Path>>(path: T, is_file: bool) -> anyhow::Result<PathBuf> {
    let path = path.as_ref();
    let is_type = |f: &Path| -> bool { if is_file { f.is_file() } else { f.is_dir() } };

    Ok(if path.exists() && is_type(path) {
        PathBuf::from(path)
    } else {
        if path.is_absolute() {
            return Err(anyhow!("init_morsels: db_path is not a file: {:?}", path));
        } else {
            let pwd = std::env::current_dir()?;
            let pb = pwd.join(path);
            let path = pb.as_path();
            if path.exists() && is_type(path) {
                PathBuf::from(path)
            } else {
                return Err(anyhow!("init_morsels: db_path is not a file: {:?}", path));
            }
        }
    })
}
