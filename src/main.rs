use log::{error, info};
use notify::{
    recommended_watcher, Event, RecursiveMode, Watcher,
    event::ModifyKind,
};
use tokio::sync::mpsc;
use anyhow::{Result};
use std::path::{Path, PathBuf};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use dotenv::dotenv;

mod utils;
use utils::{has_content_changed, is_ignored_path};

mod diff;
use crate::diff::compute_text_edits;
mod llm;
use llm::LlmClient;
mod prompts;
mod coder;
use coder::{Coder, CURSOR_MARKER};
mod state;
use state::{State, SharedState, FileState};
mod config;
use config::{Config, init_logger};

fn log_create_event(path: &Path) {
    info!("watcher:create {:?}", (path, path.is_file()));
}

fn log_remove_event(path: &Path) {
    info!("watcher:remove {:?}", (path, path.is_file()));
}

fn log_content_change(path: &Path, old: Option<&String>, new: &str) {
    match old {
        Some(old) => {
            info!("File {:?} updated", path);
            let diffs = compute_text_edits(old, new);
            for d in diffs { info!("{:?}", d) }
        }
        None => info!("File {:?} added with content:\n{}", path, new),
    }
}

async fn handle_modify_event(
    path: &PathBuf, state: SharedState
) -> Result<()> {
    info!("watcher:modify {:?}", (path, path.is_file()));

    let new_content = tokio::fs::read_to_string(path).await?;
    info!("watcher:new_content {:?}", new_content);

    let mut state = state.write().await;
    let maybe_old_content = state.file2state.get(path).map(|fs| &fs.content);

    if !has_content_changed(maybe_old_content, &new_content) {
        info!("watcher:content_unchanged {:?}", path);
        return Ok(());
    }

    log_content_change(path, maybe_old_content, &new_content);

    let final_content = if let Some(pos) = new_content.find(CURSOR_MARKER) {
        let updated = state.coder.autocomplete(&new_content, path, pos).await?;
        write(&path, &updated).await?;
        updated
    } else {
        info!("No {} found in file {:?}", CURSOR_MARKER, path);
        new_content
    };

    state.file2state.insert(path.clone(), FileState {
        content: final_content,
    });

    Ok(())
}

async fn write(path: &PathBuf, content: &String) -> Result<()> {
    tokio::fs::write(path, content).await?;
    Ok(())
}

async fn process_path(
    path: PathBuf,
    event: notify::Event,
    shared_state: SharedState,
    in_flight: &mut HashMap<PathBuf, JoinHandle<()>>,
) {
    match event.kind {
        notify::EventKind::Create(_) => log_create_event(&path),
        notify::EventKind::Remove(_) => log_remove_event(&path),
        notify::EventKind::Modify(ModifyKind::Data(_)) => {
            
            if let Some(handle) = in_flight.remove(&path) {
                handle.abort();
            }
        
            let state = shared_state.clone();
            let path_clone = path.clone();
        
            let handle = tokio::spawn(async move {
                let start_time = std::time::Instant::now();
                
                let res = handle_modify_event(&path_clone, state).await;
                if let Err(e) = res {
                    error!("Error handling event for {:?}: {}", path_clone, e);
                }
                let elapsed = start_time.elapsed();
                info!("Done handling event for {:?} in {:?}", path_clone, elapsed);
            });
        
            in_flight.insert(path, handle);
        }
        _ => { }
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    init_logger();

    let config = Config::from_env()?;
    let Config { api_key, base_url, model } = config;
    
    let client = LlmClient::new(&api_key, &base_url, &model);
    let coder = Coder::new(client);
    
    let state = State::new(coder);
    let shared_state: SharedState = Arc::new(RwLock::new(state));

    let (watch_tx, mut watch_rx) = mpsc::channel::<notify::Result<Event>>(32);
    let mut watcher = recommended_watcher(move |res| {
        let _ = watch_tx.blocking_send(res);
    })?;

    let dir = Path::new(".");
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
                    .filter(|path| !is_ignored_path(path))
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
