use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::coder::Coder;

/// Represents the state of a single file
#[derive(Debug, Clone)]
pub struct FileState {
    pub content: String,
}

/// Global application state
pub struct State {
    pub file2state: HashMap<PathBuf, FileState>,
    pub coder: Coder,
}

/// Shared state wrapped in Arc<RwLock> for thread-safe access
pub type SharedState = Arc<RwLock<State>>;

impl State {
    pub fn new(coder: Coder) -> Self {
        Self {
            file2state: HashMap::new(),
            coder,
        }
    }
}