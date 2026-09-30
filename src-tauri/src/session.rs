//! In-memory capture sessions. Images never touch disk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::capture::Rect;

// Read by the LLM loop in v0.2.
#[allow(dead_code)]
pub struct Session {
    pub png: Vec<u8>,
    pub rect: Rect,
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

    pub fn remove(&self, id: &str) -> bool {
        self.inner.lock().unwrap().remove(id).is_some()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}
