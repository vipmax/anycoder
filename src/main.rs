use log::{debug, error, info, warn};
use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
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

struct State {
    file2state: HashMap<PathBuf, FileState>,
    coder: Coder,
}

type SharedState = Arc<RwLock<State>>;

fn init_logger() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();
}

async fn handle_watch_event(
    path: &PathBuf, event: &notify::Event, state: SharedState
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
            handle_modify_event(path, state).await?;
        }
        _ => {}
    }

    Ok(())
}

fn log_create_event(path: &Path) {
    info!("watcher:create {:?}", (path, path.is_file()));
}

fn log_remove_event(path: &Path) {
    info!("watcher:remove {:?}", (path, path.is_file()));
}

async fn handle_modify_event(
    path: &PathBuf, state: SharedState
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
    let old_content_opt = map.file2state.get(path).map(|fs| &fs.content);

    if !has_content_changed(old_content_opt, &new_content) {
        return Ok(());
    }

    log_content_change(path, old_content_opt, &new_content);

    let final_content = if let Some(pos) = new_content.find(CURSOR_MARKER) {
        let updated = map.coder.autocomplete(&new_content, path, pos).await;
        match updated {
            Ok(updated) => {
                if let Err(e) = tokio::fs::write(path, &updated).await {
                    warn!("Failed to write file {:?}: {}", path, e);
                }
                updated
            }
            Err(e) => {
                warn!("Failed to autocomplete file {:?}: {}", path, e);
                new_content
            }
        }
    } else {
        info!("No {} found in file {:?}", CURSOR_MARKER, path);
        new_content
    };

    map.file2state.insert(path.clone(), FileState {
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

fn log_content_change(path: &Path, old: Option<&String>, new: &str) {
    match old {
        Some(old) => {
            info!("File {:?} updated", path);
            let diffs = crate::diff::compute_text_edits(old, new);
            for d in diffs {
                info!("{:?}", d);
            }
        }
        None => info!("File {:?} added with content:\n{}", path, new),
    }
}

async fn process_path(
    path: PathBuf,
    event: notify::Event,
    shared_state: SharedState,
    in_flight: &mut HashMap<PathBuf, JoinHandle<()>>,
) {
    if let Some(handle) = in_flight.remove(&path) {
        handle.abort();
    }

    let state = shared_state.clone();
    let event = event.clone();
    let path_clone = path.clone();

    let handle = tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        
        let res = handle_watch_event(&path_clone, &event, state).await;
        if let Err(e) = res {
            error!("Error handling event for {:?}: {}", path_clone, e);
        }
        let elapsed = start_time.elapsed();
        info!("Done handling event for {:?} in {:?}", path_clone, elapsed);
    });

    in_flight.insert(path, handle);
}


#[tokio::main]
async fn main() -> Result<()> {
    dotenv()?;
    init_logger();

    let api_key = std::env::var("OPENROUTER_API_KEY")?;
    let base_url = "https://openrouter.ai/api/v1";
    let model = "mistralai/codestral-2501";

    let client = LlmClient::new(&api_key, base_url, model);
    let coder = Coder::new(client);

    let shared_state: SharedState = Arc::new(RwLock::new(State {
        file2state: HashMap::new(),
        coder,
    }));

    let (watch_tx, mut watch_rx) = mpsc::channel::<notify::Result<Event>>(32);
    let mut watcher = recommended_watcher(move |res| {
        let _ = watch_tx.blocking_send(res);
    })?;

    let dir = Path::new("..");
    watcher.watch(dir, RecursiveMode::Recursive)?;

    info!("Starting anycoder");
    info!("I'll help you to code.");
    info!("All you need is to write {} wherever you want", CURSOR_MARKER);
    info!("Watching files at {:?}", dir);

    let mut in_flight: HashMap<PathBuf, JoinHandle<()>> = HashMap::new();

    while let Some(res) = watch_rx.recv().await {
        match res {
            Ok(event) => {
                
                let filtered_paths: Vec<PathBuf> = event.paths.iter()
                    .filter(|path| !is_ignored_dir(path))
                    .cloned().collect(); 
                
                for path in filtered_paths {
                    process_path(
                        path, 
                        event.clone(), 
                        shared_state.clone(), 
                        &mut in_flight
                    ).await;
                }
            }
            Err(e) => error!("watch error: {:?}", e),
        }
    }

    Ok(())
}
