use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A qualification captured *before* the visitor confirmed their email.
/// Mirrors the chat `qualify` block so the pipeline can be spawned verbatim
/// once the email is confirmed.
#[derive(Clone)]
pub struct PendingQualification {
    pub client_need: String,
    pub summary: String,
}

/// Per-session lifecycle of a gated pipeline.
enum Pending {
    /// Qualified but not yet unlocked — waiting on the double opt-in email.
    Qualify(PendingQualification),
    /// Email confirmed and the pipeline spawned — carries its id so the polling
    /// frontend can pick it up via `/api/auth/status`.
    Spawned(String),
}

/// In-memory hand-off between the chat handler (which stashes a qualification
/// when the visitor isn't unlocked yet) and the confirm handler (which spawns
/// the pipeline after the email is verified). Keyed by chat `session_id`.
///
/// Memory only: if the process restarts before the user clicks the confirm
/// link, they simply re-qualify. The volume is a demo's worth of leads, so the
/// extra DB table isn't worth it.
#[derive(Clone, Default)]
pub struct PendingStore {
    inner: Arc<RwLock<HashMap<String, Pending>>>,
}

impl PendingStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stash a qualification awaiting email confirmation. Overwrites any prior
    /// pending qualification for the session (the latest qualify block wins).
    pub async fn stash(&self, session_id: String, qualification: PendingQualification) {
        self.inner
            .write()
            .await
            .insert(session_id, Pending::Qualify(qualification));
    }

    /// Remove and return the stashed qualification for a session, if one is
    /// awaiting confirmation. Returns `None` if there's nothing pending or it
    /// was already spawned — so the caller spawns at most once.
    pub async fn take_qualify(&self, session_id: &str) -> Option<PendingQualification> {
        let mut w = self.inner.write().await;
        match w.get(session_id) {
            Some(Pending::Qualify(_)) => match w.remove(session_id) {
                Some(Pending::Qualify(q)) => Some(q),
                _ => None,
            },
            _ => None,
        }
    }

    /// Record the pipeline id spawned for a session after its email was
    /// confirmed, so the polling frontend can pick it up.
    pub async fn set_spawned(&self, session_id: String, pipeline_id: String) {
        self.inner
            .write()
            .await
            .insert(session_id, Pending::Spawned(pipeline_id));
    }

    /// The pipeline id spawned for this session, if any.
    pub async fn spawned_id(&self, session_id: &str) -> Option<String> {
        match self.inner.read().await.get(session_id) {
            Some(Pending::Spawned(id)) => Some(id.clone()),
            _ => None,
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no network, no database
// Covers  : stash → take_qualify (spawn-once), set_spawned → spawned_id,
//           and that the two lifecycle phases don't leak into each other
#[cfg(test)]
mod tests {
    use super::*;

    fn qual() -> PendingQualification {
        PendingQualification {
            client_need: "sync Shopify → Sage".to_string(),
            summary: "80 orders/day, manual re-entry".to_string(),
        }
    }

    #[tokio::test]
    async fn take_qualify_returns_stashed_then_none() {
        let store = PendingStore::new();
        store.stash("sess".to_string(), qual()).await;

        let taken = store.take_qualify("sess").await.expect("must return stash");
        assert_eq!(taken.client_need, "sync Shopify → Sage");
        // Second take must be None — guarantees we spawn at most once.
        assert!(store.take_qualify("sess").await.is_none());
    }

    #[tokio::test]
    async fn take_qualify_none_for_unknown_session() {
        let store = PendingStore::new();
        assert!(store.take_qualify("nobody").await.is_none());
    }

    #[tokio::test]
    async fn spawned_id_round_trips() {
        let store = PendingStore::new();
        assert!(store.spawned_id("sess").await.is_none());
        store.set_spawned("sess".to_string(), "pipe-123".to_string()).await;
        assert_eq!(store.spawned_id("sess").await, Some("pipe-123".to_string()));
    }

    #[tokio::test]
    async fn spawned_entry_is_not_taken_as_qualify() {
        let store = PendingStore::new();
        store.set_spawned("sess".to_string(), "pipe-9".to_string()).await;
        // A spawned entry must not be consumed by take_qualify (no re-spawn).
        assert!(store.take_qualify("sess").await.is_none());
        assert_eq!(store.spawned_id("sess").await, Some("pipe-9".to_string()));
    }

    #[tokio::test]
    async fn stash_then_spawn_overwrites_phase() {
        let store = PendingStore::new();
        store.stash("sess".to_string(), qual()).await;
        let q = store.take_qualify("sess").await.unwrap();
        store.set_spawned("sess".to_string(), "pipe-1".to_string()).await;
        // After spawning, the qualify phase is gone and the id is queryable.
        assert!(store.take_qualify("sess").await.is_none());
        assert_eq!(store.spawned_id("sess").await, Some("pipe-1".to_string()));
        let _ = q;
    }
}
