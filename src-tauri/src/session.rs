//! In-memory capture sessions: the prepared image plus its conversation thread.
//! Images never touch disk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;

use crate::capture::Rect;
use crate::preprocess::PreparedImage;
use crate::provider::{Classification, Message, Usage};

pub struct Session {
    pub image: Arc<PreparedImage>,
    #[allow(dead_code)] // overlay placement for follow-up windows
    pub rect: Rect,
    pub classification: Option<Classification>,
    pub history: Vec<Message>,
    pub last_answer: Option<String>,
    pub totals: Totals,
    /// The in-flight answer stream, aborted when the session closes.
    pub task: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl Totals {
    pub fn add(&mut self, u: &Usage) {
        self.input_tokens += u.input_tokens;
        self.output_tokens += u.output_tokens;
        self.cache_read_input_tokens += u.cache_read_input_tokens;
        self.cache_creation_input_tokens += u.cache_creation_input_tokens;
    }
}

impl Session {
    pub fn new(image: PreparedImage, rect: Rect) -> Self {
        Self {
            image: Arc::new(image),
            rect,
            classification: None,
            history: Vec::new(),
            last_answer: None,
            totals: Totals::default(),
            task: None,
        }
    }

    pub fn busy(&self) -> bool {
        self.task.as_ref().is_some_and(|t| !t.inner().is_finished())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[derive(Default)]
pub struct Sessions {
    inner: Mutex<HashMap<String, Session>>,
    counter: AtomicU64,
}

impl Sessions {
    pub fn insert(&self, session: Session) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let id = format!("s{millis:x}{n:x}");
        self.inner.lock().unwrap().insert(id.clone(), session);
        id
    }

    /// Run `f` against a live session, if it still exists.
    pub fn with<R>(&self, id: &str, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
        self.inner.lock().unwrap().get_mut(id).map(f)
    }

    pub fn remove(&self, id: &str) -> bool {
        self.inner.lock().unwrap().remove(id).is_some()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}
