use crate::async_tasks::{Named, Trigrams};
use crate::{DIRECTORY_TRIGRAMS, DirectorySource, PluginConfig};
use anyhow::anyhow;
use log::debug;
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

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
    pub url_path: String,
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
        dir_config
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
            if path.exists() && path.is_file() {
                debug!("init_morsels: file exists:  {:?}", path.display());
                let mut db_file = File::open(path).await?;
                let mut buffer = String::new();
                let _ = db_file.read_to_string(&mut buffer).await?;
                serde_json::from_str::<DirectoryFormat>(buffer.as_str())?
            } else {
                let db_path = match std::env::current_dir() {
                    Ok(pwd) => pwd.join(path.as_os_str()).display().to_string(),
                    Err(e) => format!("NO PWD: {e:?}"),
                };
                return Err(anyhow!("directory db not found in {}", db_path));
            }
        }
    };

    let trigrams = Trigrams::new(directory.directory)?;
    debug!("init_directory: trigrams: {:?}", trigrams);
    let mut tgms = DIRECTORY_TRIGRAMS
        .write()
        .map_err(|e| anyhow!(e.to_string()))?;
    *tgms = Some(trigrams);
    Ok(true)
}
