//! keywords plugin. Find information for agents via fuzzy keyword search

use fs2::FileExt;
//
use mcp_plugin_api::*;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

mod async_tasks;
use async_tasks::{Trigrams, run_async_tasks};

#[derive(Serialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MorselResponse {
    Success {
        results_count: usize,
        morsels: Vec<MorselResult>,
    },
    NoMatch {
        searched_keywords: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },
}

#[derive(Serialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LinkResponse {
    Success {
        results_count: usize,
        links: Vec<LinkResult>,
    },
    NoMatch {
        searched_keywords: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FailLogEntry {
    keyword: String,
    kw_type: String,
    count: usize,
    timestamp: String,
}

static DIRECTORY_INFO: Lazy<RwLock<Option<Arc<DirectoryInfo>>>> = Lazy::new(|| RwLock::new(None));

static MORSEL_TRIGRAMS: Lazy<RwLock<Option<Trigrams<MorselEntry>>>> =
    Lazy::new(|| RwLock::new(None));

static FAILED_KEYWORDS: Lazy<RwLock<Option<HashMap<(String, String), FailLogEntry>>>> =
    Lazy::new(|| RwLock::new(None));

// ============================================================================
// Configuration
// ============================================================================
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct KeywordsConfig {
    /// set to false to deactivate
    #[serde(default = "active")]
    active: bool,
    /// Function description for 'keywords_to_morsel' tool
    function_descr: String,
    /// Path to Directory File
    db_path: PathBuf,
    /// Maximum number of candidates to retrieve in fuzzy card name search
    #[schemars(range(min = 1, max = 10))]
    #[serde(default = "n_best")]
    n_best: usize,
    /// Minimum score for a candidate in fuzzy card name search to make it to the result list
    #[schemars(range(min = 0.0, max = 1.0))]
    #[serde(default = "min_score")]
    min_score: f64,
    /// Suggestion on nothing found
    suggestion: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DirectorySource {
    Http(String),
    Local(PathBuf),
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DirectoryConfig {
    /// set to false to deactivate
    #[serde(default = "active")]
    active: bool,
    /// Function description for directory tool
    function_descr: String,
    /// Path to Directory File
    source: DirectorySource,
    /// Path to failed keyword log
    #[schemars(range(min = 1, max = 10))]
    #[serde(default = "n_best")]
    n_best: usize,
    /// Minimum score for a candidate in fuzzy card name search to make it to the result list
    #[schemars(range(min = 0.0, max = 1.0))]
    #[serde(default = "min_score")]
    min_score: f64,
    /// Suggestion on nothing found
    suggestion: Option<String>,
}

/// Plugin configuration structure
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PluginConfig {
    /// Path to failed keyword log
    failed_keywords_path: Option<PathBuf>,
    /// Keywords configuration
    keywords: Option<KeywordsConfig>,
    /// Directory configuration
    directory: Option<DirectoryConfig>,
    #[schemars(range(min = 120))]
    #[serde(default = "default_update_interval_secs")]
    update_interval_secs: u32,
}

fn n_best() -> usize {
    1
}

fn min_score() -> f64 {
    0.2
}

fn active() -> bool {
    true
}
fn default_update_interval_secs() -> u32 {
    3600
}

// Generate all configuration boilerplate with one macro!
declare_plugin_config!(PluginConfig);

use crate::async_tasks::{DirectoryInfo, LinkResult, MorselEntry, MorselResult};
use env_logger::{Builder, Target};
use log::{debug, error, info, warn};

fn init() -> Result<(), String> {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stderr);
    builder.init();
    info!("Initializing ConverZen keywords MCP plugin");
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = env!("GIT_HASH");
    let build_ts = env!("BUILD_TIMESTAMP");
    info!("version {version}, Rev. {git_hash}, build-ts: {build_ts}");
    run_async_tasks().map_err(|e| e.to_string())
}

// ============================================================================
// Tool Handlers
// ============================================================================

// Get directory information for website
fn handle_get_link(args: &Value) -> Result<Value, String> {
    debug!("keywords_to_link called with args: {args:?}");
    let config = get_config();
    let dir_config = if let Some(dir_config) = &config.directory {
        if !dir_config.active {
            return Err(String::from("keywords_to_link is deactivated"));
        }
        dir_config
    } else {
        return Err(String::from("keywords_to_link is not configured"));
    };

    let directory_info = DIRECTORY_INFO
        .read()
        .map_err(|e| format!("cannot read directory entries: {e}"))?
        .clone()
        .ok_or("Directory data is not initialized")?;

    let keywords = args["keywords"]
        .as_str()
        .ok_or("Missing or invalid parameter 'keywords'")?;

    let keywords = keywords
        .split([',', ' '])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut failed_keywords = if config.failed_keywords_path.is_some() {
        Some(vec![])
    } else {
        None
    };

    let matches = directory_info.trigrams.search_many(
        &keywords,
        dir_config.n_best,
        dir_config.min_score,
        &mut failed_keywords,
    );

    if let Some(failed_keywords) = failed_keywords {
        log_failed_keywords(&failed_keywords, config, "directory");
    }

    // .join() handles the leading slash in "/here" automatically
    let md_content = if !matches.is_empty() {
        let mut links = Vec::with_capacity(matches.len());
        for dir_match in matches {
            let item = dir_match.item;
            links.push(LinkResult {
                url: directory_info
                    .origin
                    .join(item.path.as_str())
                    .map_err(|e| e.to_string())?
                    .to_string(),
                title: item.title,
                score: dir_match.score as f32,
                description: item.description,
            });
        }
        LinkResponse::Success {
            results_count: links.len(),
            links,
        }
    } else {
        LinkResponse::NoMatch {
            searched_keywords: keywords.iter().map(|s| s.to_string()).collect(),
            suggestion: dir_config.suggestion.clone(),
        }
    };

    debug!("handle_get_morsel: returning: {md_content:?}");

    // Return structured JSON data for programmatic clients
    Ok(utils::json_content(
        serde_json::to_value(md_content).map_err(|e| e.to_string())?,
    ))
}

/// keyword to information morsel lookup
fn handle_get_morsel(args: &Value) -> Result<Value, String> {
    debug!("keywords_to_morsel called with args: {args:?}");
    let config = get_config();
    let kwd_config = if let Some(kwd_config) = &config.keywords {
        if !kwd_config.active {
            return Err(String::from("keywords_to_morsel is deactivated"));
        }
        kwd_config
    } else {
        return Err(String::from("keywords_to_morsel is not configured"));
    };

    let keywords = args["keywords"]
        .as_str()
        .ok_or("Missing or invalid parameter 'keywords'")?;

    let keywords = keywords
        .split([',', ' '])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut failed_keywords = if config.failed_keywords_path.is_some() {
        Some(vec![])
    } else {
        None
    };

    let matches = MORSEL_TRIGRAMS
        .read()
        .map_err(|e| format!("cannot read directory entries: {e}"))?
        .as_ref()
        .ok_or("Morsel data is not initialized")?
        .search_many(
            &keywords,
            kwd_config.n_best,
            kwd_config.min_score,
            &mut failed_keywords,
        );

    if let Some(failed) = failed_keywords {
        if !failed.is_empty() {
            log_failed_keywords(&failed, config, "keywords");
        }
    }

    let md_content = if !matches.is_empty() {
        let mut morsels = Vec::with_capacity(matches.len());
        matches.into_iter().for_each(|m| {
            let item = m.item;

            morsels.push(MorselResult {
                id: item.id.clone(),
                content: item.content.clone(),
                links: item.links.clone(),
                score: m.score as f32,
            });
        });
        MorselResponse::Success {
            results_count: morsels.len(),
            morsels,
        }
    } else {
        MorselResponse::NoMatch {
            searched_keywords: keywords.iter().map(|s| s.to_string()).collect(),
            suggestion: kwd_config.suggestion.clone(),
        }
    };

    debug!("handle_get_morsel: returning: {md_content:?}");

    // Return structured JSON data for programmatic clients
    Ok(utils::json_content(
        serde_json::to_value(md_content).map_err(|e| e.to_string())?,
    ))
}

pub fn log_failed_keywords(keywords: &[String], config: &PluginConfig, kw_type: &str) {
    if keywords.is_empty() {
        return;
    }
    let mut updated = false;
    match FAILED_KEYWORDS.write() {
        Ok(mut failed_keywords) => {
            if let Some(failed_keywords) = &mut *failed_keywords {
                updated = true;
                for keyword in keywords {
                    failed_keywords
                        .entry((keyword.to_owned(), kw_type.to_string()))
                        .and_modify(|entry| {
                            entry.count += 1;
                            entry.timestamp = chrono::Utc::now().to_string();
                        })
                        .or_insert(FailLogEntry {
                            keyword: keyword.clone(),
                            kw_type: kw_type.to_string(),
                            count: 1,
                            timestamp: chrono::Utc::now().to_string(),
                        });
                }
            } else {
                error!("failed keywords are not initialized");
            }
        }
        Err(e) => {
            error!("cannot access failed keywords: {e}");
        }
    }
    if let Some(path) = &config.failed_keywords_path {
        if !updated {
            return;
        }
        let mut file = match OpenOptions::new()
            .read(true) // Required for Windows locking
            .write(true) // Atomic pointer positioning
            .create(true) // Create if missing
            .open(path)
        {
            Ok(file) => file,
            Err(e) => {
                error!("cannot open failed file for update: {e}");
                return;
            }
        };

        // 2. Acquire an exclusive lock (blocks the thread until free)
        match file.lock_exclusive() {
            Ok(_) => {
                let kwd_entries = match FAILED_KEYWORDS.read() {
                    Ok(failed_keywords) => {
                        if let Some(failed_keywords) = &*failed_keywords {
                            failed_keywords.values().cloned().collect::<Vec<_>>()
                        } else {
                            vec![]
                        }
                    }
                    Err(e) => {
                        error!("cannot access failed keywords: {e}");
                        return;
                    }
                };

                let data = match serde_json::to_string(&kwd_entries) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("cannot serialize failed keywords: {e}");
                        let _ = file
                            .unlock()
                            .inspect_err(|e| error!("cannot unlock file: {e}"));
                        return;
                    }
                };
                let _ = file
                    .write_all(data.as_bytes())
                    .inspect_err(|e| error!("cannot write to file: {e}"));
                let _ = file
                    .sync_all()
                    .inspect_err(|e| error!("cannot sync file: {e}"));
                let _ = file
                    .unlock()
                    .inspect_err(|e| error!("cannot unlock file: {e}"));
            }
            Err(e) => {
                warn!("file is locked: {e}");
            }
        }
    }
}

// ============================================================================
// Plugin Declaration
// ============================================================================

// Declare tools using the standard macro
// Async handlers are wrapped with wrap_async_handler!

// When the MCP server is called with --get-plugin-schema it retrieves
// the tool list before the plugin is configured so get_config will fail
// dramatically so we we need to use try_get_config() and supply a default
// for the function description

declare_tools! {
    tools: [
        (|| match try_get_config() {
        Some(config) => {
            if let Some(kwd_cfg) = &config.keywords {
                Tool::builder("keywords_to_morsel",kwd_cfg.function_descr.as_str(), kwd_cfg.active)
                    .param_string("keywords", "Comma separated list of keywords", true)
                    .handler(handle_get_morsel)
            } else {
                Tool::builder("keywords_to_morsel","N/A", false)
                    .param_string("keywords", "Comma separated list of keywords", true)
                    .handler(handle_get_morsel)
            }
        }
        None =>
            Tool::builder("keywords_to_morsel","N/A - not configured yet", true)
                .param_string("keywords", "Comma separated list of keywords", true)
                .handler(handle_get_morsel)
    })(),
        (|| match try_get_config() {
        Some(config) => {
            if let Some(dir_cfg) = &config.directory {
                Tool::builder("keywords_to_link",dir_cfg.function_descr.as_str(), dir_cfg.active)
                    .param_string("keywords", "Comma separated list of keywords", true)
                    .handler(handle_get_link)
            } else {
                Tool::builder("keywords_to_link","N/A", false)
                    .param_string("keywords", "Comma separated list of keywords", true)
                    .handler(handle_get_link)
            }
        }
        None =>
            Tool::builder("keywords_to_link","N/A - not configured yet", true)
                .param_string("keywords", "Comma separated list of keywords", true)
                .handler(handle_get_link)
    })()
    ]
}
declare_config_schema!(PluginConfig);
declare_plugin_init!(init);
// Declare the plugin with auto-generated functions and configuration
declare_plugin! {
    list_tools: generated_list_tools,
    execute_tool: generated_execute_tool,
    free_string: utils::standard_free_string,
    configure: plugin_configure,
    init: plugin_init,
    get_config_schema: plugin_get_config_schema
}
