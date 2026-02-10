use crate::{FailLogEntry, PluginConfig, FAILED_KEYWORDS, MORSEL_TRIGRAMS};
use anyhow::anyhow;
use log::{debug, error, warn};
use super::trigrams::{Named, Trigrams};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::BufReader;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorselEntry {
    pub id: String,
    pub keywords: Vec<String>,
    pub content: String,
    pub link: Option<String>,
}

impl Named for MorselEntry {
    fn names(&self) -> &[String] {
        self.keywords.as_slice()
    }
}

pub async fn init_morsels(config: &PluginConfig) -> anyhow::Result<()> {
    debug!("init_directory: config: {config:?}");
    let db_path = config.database_path.as_path();
    if db_path.exists() && db_path.is_file() {
        debug!("init_directory: file exists:  {:?}", config.database_path);
        let mut db_file = File::open(db_path).await?;
        let mut buffer = String::new();
        let bytes_read = db_file.read_to_string(&mut buffer).await?;
        debug!("init_directory: bytes read:   {bytes_read}");
        let entries: Vec<MorselEntry> = serde_yaml::from_str(buffer.as_str())?;
        debug!("init_directory: parsed {} entries", entries.len());
        let trigrams = Trigrams::new(entries)?;
        debug!("init_directory: trigrams");
        let mut tgms = MORSEL_TRIGRAMS
            .write()
            .map_err(|e| anyhow!(e.to_string()))?;
        *tgms = Some(trigrams);
        Ok(())
    } else {
        let database_path = match std::env::current_dir() {
            Ok(path) => path
                .join(config.database_path.as_os_str())
                .display()
                .to_string(),
            Err(e) => format!("NO PWD: {e:?}"),
        };
        let message = format!(
            "init_directory: database path does not exist or is not a file: {} -> {}",
            config.database_path.display(),
            database_path
        );

        error!("{}", message.as_str());
        Err(anyhow!(message))
    }
}

pub async fn init_failed_keywords(config: &PluginConfig) -> anyhow::Result<()> {
    if let Some(path) = config.failed_keywords_path.as_ref() {
        debug!("log_failed_keywords: path: {}", path.display());

        match FAILED_KEYWORDS.read() {
            Ok(keywords) => {
                if keywords.is_some() {
                    debug!("log_failed_keywords: keyword store already initialized");
                    return Ok(());
                }
            }
            Err(e) => {
                error!("cannot access keywords: {e}");
                return Err(anyhow!("cannot access keywords: {e}"));
            }
        };

        let log_file = match OpenOptions::new().read(true).open(path) {
            Ok(log_file) => log_file,
            Err(_e) => {
                let log_path = match std::env::current_dir() {
                    Ok(path) => path.join(path.as_os_str()).display().to_string(),
                    Err(e) => format!("NO PWD: {e:?}"),
                };
                warn!("log_failed_keywords: log_path can not be opened: {log_path}");

                let mut fkwd = FAILED_KEYWORDS
                    .write()
                    .map_err(|e| anyhow!(e.to_string()))?;
                *fkwd = Some(HashMap::new());
                return Ok(());
            }
        };

        let reader = BufReader::new(log_file);
        let entries: Vec<FailLogEntry> = match serde_json::from_reader(reader) {
            Ok(entries) => entries,
            Err(e) => {
                error!("log_failed_keywords: failed to parse log file: {e}");
                return Err(anyhow!(
                    "log_failed_keywords: failed to parse log file: {e}"
                ));
            }
        };
        let mut kwd_map = HashMap::new();
        entries.into_iter().for_each(|entry| {
            kwd_map.insert(entry.keyword.clone(), entry);
        });

        let mut kwd_store = FAILED_KEYWORDS
            .write()
            .map_err(|e| anyhow!(e.to_string()))?;

        if kwd_store.is_some() {
            Err(anyhow!("keyword store already initialized"))
        } else {
            *kwd_store = Some(kwd_map);
            Ok(())
        }
    } else {
        Ok(())
    }
}
