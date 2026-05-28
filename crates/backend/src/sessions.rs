use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const FREE_MESSAGES: u32 = 5;

#[derive(Default)]
pub struct Session {
    pub message_count: u32,
    pub unlocked: bool,
    pub email: Option<String>,
}

#[derive(Clone)]
pub struct SessionStore(pub Arc<RwLock<HashMap<String, Session>>>);

impl SessionStore {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    // Returns true if the session is allowed to send a message
    pub async fn check_and_increment(&self, key: &str) -> bool {
        let mut store = self.0.write().await;
        let session = store.entry(key.to_string()).or_default();
        if session.unlocked || session.message_count < FREE_MESSAGES {
            session.message_count += 1;
            true
        } else {
            false
        }
    }

    pub async fn unlock_with_email(&self, key: &str, email: String) -> bool {
        let mut store = self.0.write().await;
        let session = store.entry(key.to_string()).or_default();
        if session.unlocked {
            return false; // already unlocked
        }
        session.unlocked = true;
        session.email = Some(email.clone());
        tracing::info!("Lead captured — email: {email} session: {key}");
        true
    }

    pub async fn message_count(&self, key: &str) -> u32 {
        self.0.read().await.get(key).map(|s| s.message_count).unwrap_or(0)
    }
}
