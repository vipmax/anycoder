use log::{debug, error, info, warn};
use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use anyhow::Result;
use std::path::PathBuf;
use std::{sync::Arc, collections::HashMap};
use tokio::sync::RwLock;
use dotenv::dotenv;

mod utils;
use utils::is_ignored_dir;

mod diff;
mod llm;
use llm::LlmClient;
mod prompts;
mod coder;
use coder::{Coder, CURSOR_MARKER};

#[derive(Debug, Clone)]
struct FileState {
    content: String,
}

type SharedFileMap = Arc<RwLock<HashMap<PathBuf, FileState>>>;
type SharedCoder = Arc<RwLock<Coder>>;


fn init_logger() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();
}

async fn handle_watch_event(
    path: &std::path::PathBuf,
    event: &notify::Event,
    state: SharedFileMap,
    coder: SharedCoder,
) -> Result<()> {
    info!("watch event: {:?}", event);

    match event.kind {
        notify::EventKind::Create(_) => {
            log_create_event(path);
        }
        notify::EventKind::Remove(_) => {
            log_remove_event(path);
        }
        notify::EventKind::Modify(notify::event::ModifyKind::Data(_)) => {
            handle_modify_event(path, state, coder).await?;
        }
        _ => {}
    }

    Ok(())
}

fn log_create_event(path: &std::path::Path) {
    info!("watcher:create {:?}", (path, path.is_file()));
}

fn log_remove_event(path: &std::path::Path) {
    info!("watcher:remove {:?}", (path, path.is_file()));
}

async fn handle_modify_event(
    path: &std::path::PathBuf, state: SharedFileMap, coder: SharedCoder
) -> Result<()> {
    info!("watcher:modify {:?}", (path, path.is_file()));

    let new_content = match tokio::fs::read_to_string(path).await {
        Ok(content) => content,
        Err(err) => {
            warn!("Failed to read file {:?}: {}", path, err);
            return Ok(());
        }
    };

    let mut map = state.write().await;
    let old_content_opt = map.get(path).map(|fs| &fs.content);

    if !has_content_changed(old_content_opt, &new_content) {
        return Ok(());
    }

    log_content_change(path, old_content_opt, &new_content);

    let final_content = if let Some(pos) = new_content.find(CURSOR_MARKER) {
        // let updated = replace_cursor_marker(pos, path, new_content).await?;
        // updated

        let coder = coder.write().await;
        let updated = coder.autocomplete(&new_content, path, pos).await;

        match updated {
            Ok(updated) => {
                std::fs::write(&path, updated.clone());
                updated
            },
            Err(e) => {
                warn!("Failed to autocomplete file {:?}: {}", path, e);
                new_content
            },
        }

    } else {
        info!("No {} found in file {:?}", CURSOR_MARKER, path);
        new_content
    };

    map.insert(path.clone(), FileState {
        content: final_content,
    });

    Ok(())
}

fn has_content_changed(old: Option<&String>, new: &str) -> bool {
    match old {
        Some(old_content) => old_content != new,
        None => true,
    }
}

fn log_content_change(
    path: &std::path::Path, old: Option<&String>, new: &str
) {
    match old {
        Some(old) => {
            info!("File {:?} updated", path);
            let diffs = crate::diff::compute_text_edits(old, new);
            for d in diffs {
                info!("{:?}", d);
            }
        }
        None => info!(
            "File {:?} added with content:\n{}",
            path, new
        ),
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    dotenv()?;
    init_logger();

    info!("Starting anycoder");

    info!("I'll help you to code.");
    info!("All you need is to write {} wherever you want", CURSOR_MARKER);

    let shared_state: SharedFileMap = Arc::new(RwLock::new(HashMap::new()));

    let api_key = std::env::var("OPENROUTER_API_KEY")?;
    let base_url = "https://openrouter.ai/api/v1";
    let model = "mistralai/codestral-2501";
    // let model = "google/gemini-2.5-flash-preview-05-20";

    let client = LlmClient::new(&api_key, base_url, model);
    let coder = Coder::new(client);
    let shared_coder = Arc::new(RwLock::new(coder));

    let (watch_tx, mut watch_rx) = mpsc::channel::<notify::Result<Event>>(32);
    let mut watcher = recommended_watcher(move |res| {
        let _ = watch_tx.blocking_send(res);
    })?;

    let dir = std::path::Path::new("..");
    watcher.watch(dir, RecursiveMode::Recursive)?;

    info!("Watching files at {:?}", dir);

    while let Some(res) = watch_rx.recv().await {
        match res {
            Ok(event) => {
                for path in &event.paths {
                    if !is_ignored_dir(path) {
                        let state_clone = shared_state.clone();
                        let coder_clone = shared_coder.clone();
                        handle_watch_event(path, &event, state_clone, coder_clone).await;
                    }
                }
            }
            Err(e) => error!("watch error: {:?}", e),
        }
    }
    Ok(())
}
