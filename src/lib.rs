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
use std::sync::RwLock;

mod async_tasks;
use async_tasks::run_async_tasks;

use plugin_utils::Trigrams;
#[derive(Serialize, Deserialize, Debug)]
pub struct DbMorsel {
    pub id: String,
    pub content: String,
    pub link: Option<String>,
    pub score: f32, // Useful for the LLM to see confidence
}

#[derive(Serialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolResponse {
    Success {
        results_count: usize,
        morsels: Vec<DbMorsel>,
    },
    NoMatch {
        searched_keywords: Vec<String>,
        suggestion: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FailLogEntry {
    keyword: String,
    count: usize,
    timestamp: String,
}

static MORSEL_TRIGRAMS: Lazy<RwLock<Option<Trigrams<MorselEntry>>>> =
    Lazy::new(|| RwLock::new(None));

static FAILED_KEYWORDS: Lazy<RwLock<Option<HashMap<String, FailLogEntry>>>> =
    Lazy::new(|| RwLock::new(None));

// ============================================================================
// Configuration
// ============================================================================

/// Plugin configuration structure
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PluginConfig {
    /// Function description for 'keywords_to_morsel' tool
    function_description: String,
    /// Path to Directory File
    database_path: PathBuf,
    /// Path to failed keyword log
    failed_keywords_path: Option<PathBuf>,
    /// Maximum number of candidates to retrieve in fuzzy card name search
    #[schemars(range(min = 1, max = 10))]
    #[serde(default = "directory_n_best")]
    morsel_n_best: Option<usize>,
    /// Minimum score for a candidate in fuzzy card name search to make it to the result list
    #[schemars(range(min = 0.0, max = 1.0))]
    #[serde(default = "directory_min_score")]
    morsel_min_score: Option<f64>,
    #[schemars(range(min = 120))]
    #[serde(default = "default_update_interval_secs")]
    update_interval_secs: Option<u32>,
}

fn directory_n_best() -> Option<usize> {
    Some(1)
}

fn directory_min_score() -> Option<f64> {
    Some(0.2)
}

fn default_update_interval_secs() -> Option<u32> {
    Some(3600)
}

// Generate all configuration boilerplate with one macro!
declare_plugin_config!(PluginConfig);

use crate::async_tasks::MorselEntry;
use env_logger::{Builder, Target};
use log::{debug, error, info, warn};

fn init() -> Result<(), String> {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stderr);
    builder.init();
    info!("initializing zeno MCP plugin",);
    run_async_tasks().map_err(|e| e.to_string())
}

// ============================================================================
// Tool Handlers
// ============================================================================

/// Directory information from website
/// Synchronous Handler for get_directory_info tool, sync/async
fn handle_get_morsel(args: &Value) -> Result<Value, String> {
    debug!("zeno_keywords_to_morsel called with args: {args:?}");
    let config = get_config();
    let keywords = args["keywords"]
        .as_str()
        .ok_or("Missing or invalid parameter 'keywords'")?;

    let keywords = keywords
        .split([',', ' '])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut failed_keywords = Vec::new();
    let mut matches = Vec::new();
    for keyword in &keywords {
        let kwd_matches = MORSEL_TRIGRAMS
            .read()
            .map_err(|e| format!("cannot read directory entries: {e}"))?
            .as_ref()
            .ok_or("Morsel data is not initialized")?
            .search(
                keyword,
                config.morsel_n_best.unwrap(),
                config.morsel_min_score.unwrap(),
            );
        if kwd_matches.is_empty() && config.failed_keywords_path.is_some() {
            failed_keywords.push(keyword.to_string());
        }
        matches.extend_from_slice(&kwd_matches);
    }

    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    if let Some(n_best) = config.morsel_n_best
        && n_best < matches.len()
    {
        matches.truncate(n_best)
    }

    if !failed_keywords.is_empty() {
        log_failed_keywords(&failed_keywords, config);
    }

    let md_content = if !matches.is_empty() {
        let mut morsels = Vec::with_capacity(matches.len());
        matches.into_iter().for_each(|m| {
            let item = m.item;

            morsels.push(DbMorsel {
                id: item.id.clone(),
                content: item.content.clone(),
                link: item.link.clone(),
                score: m.score as f32,
            });
        });
        ToolResponse::Success {
            results_count: morsels.len(),
            morsels,
        }
    } else {
        ToolResponse::NoMatch {
            searched_keywords: keywords.iter().map(|s| s.to_string()).collect(),
            suggestion: "Try searching for broader terms like 'security' or 'api'.".into(),
        }
    };

    debug!("handle_get_morsel: returning: {md_content:?}");

    // Return structured JSON data for programmatic clients
    Ok(utils::json_content(
        serde_json::to_value(md_content).map_err(|e| e.to_string())?,
    ))
}

pub fn log_failed_keywords(keywords: &[String], config: &PluginConfig) {
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
                        .entry(keyword.to_owned())
                        .and_modify(|entry| {
                            entry.count += 1;
                            entry.timestamp = chrono::Utc::now().to_string();
                        })
                        .or_insert(FailLogEntry {
                            keyword: keyword.clone(),
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
    if let Some(path) = &config.failed_keywords_path
        && updated
    {
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
declare_tools! {
    tools: [
        Tool::builder("keywords_to_morsel",
r#"Use this tool to retrieve verified, high-priority information about specific product
 topics including pricing, security, technical stack, and feature shortcuts. This tool
 is faster and more accurate than a general knowledge base search for direct user inquiries.
 Input should be 1-2 core keywords (e.g., 'pricing', 'encryption', 'gdpr')."#)
            .param_string("keywords", "Comma separated list of keywords", true)
            .handler(handle_get_morsel),
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
