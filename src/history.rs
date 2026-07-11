use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

const MAX_ENTRIES: usize = 500;
const HISTORY_FILE: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub title: String,
    pub station: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
