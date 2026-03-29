use crate::async_tasks::{Named, Trigrams, validate_path};
use crate::{DIRECTORY_INFO, DirectorySource, PluginConfig};
use anyhow::anyhow;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use url::Url;

pub struct DirectoryInfo {
    pub origin: Url,
    pub trigrams: Trigrams<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryFormat {
    origin: String,
    version: String,
    directory: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinkResult {
    pub url: String,
    pub title: String,
    pub description: String,
    pub score: f32, // Useful for the LLM to see confidence
}

impl Named for DirectoryEntry {
    fn names(&self) -> &[String] {
        self.tags.as_slice()
    }
}

pub async fn init_directory(config: &PluginConfig) -> anyhow::Result<bool> {
    debug!("init_directory: config: {config:?}");
    let dir_config = if let Some(dir_config) = &config.directory {
        if dir_config.active {
            dir_config
        } else {
            return Ok(false);
        }
    } else {
        return Ok(false);
    };

    let directory = match &dir_config.source {
        DirectorySource::Http(url) => {
            let client = reqwest::Client::new();

            // 3. Fetch and parse in one go
            let response = client
                .get(url)
                .header("User-Agent", "My-Rust-MCP-Server/1.0") // Good practice for Nginx logs
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(anyhow!(format!(
                    "GET request to {url} returned status: {}, {}",
                    response.status(),
                    response.text().await?
                ))
                .into());
            }
            response.json::<DirectoryFormat>().await? // Automatically deserializes JSON to your Vec<Struct>
        }
        DirectorySource::Local(path) => {
            let path = validate_path(&path, true)?;
            debug!("init_directory: file exists:  {:?}", path.display());
            let mut db_file = File::open(path).await?;
            let mut buffer = String::new();
            let _ = db_file.read_to_string(&mut buffer).await?;
            serde_json::from_str::<DirectoryFormat>(buffer.as_str())?
        }
    };
    info!("Read Directory Info from {:?}", dir_config.source);
    info!(
        "origin: {:?}, Version: {}, {} entries",
        directory.origin,
        directory.version,
        directory.directory.len()
    );

    // .join() handles the leading slash in "/here" automatically

    let trigrams = Trigrams::new(directory.directory)?;
    let directory_info = Arc::new(DirectoryInfo {
        origin: Url::parse(directory.origin.as_str())
            .map_err(|e| anyhow!("failed to parser origin as URL: {e}"))?,
        trigrams: trigrams,
    });

    debug!("init_directory info:  {:?}", directory_info.origin);
    let mut guard = DIRECTORY_INFO.write().map_err(|e| anyhow!(e.to_string()))?;
    *guard = Some(directory_info);
    Ok(true)
}
