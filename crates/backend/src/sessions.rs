use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};

pub const FREE_MESSAGES: u32 = 5;

type HmacSha256 = Hmac<Sha256>;

#[derive(Default)]
pub struct Session {
    pub message_count: u32,
    pub unlocked: bool,
    pub email: Option<String>,
}

#[derive(Clone)]
pub struct SessionStore {
    /// Primary sessions keyed by session_id (UUID or signed token).
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    /// Secondary rate limit keyed by SHA-256(ip|fingerprint).
    /// Prevents localStorage-clearing from resetting the free tier.
    fp_limits: Arc<RwLock<HashMap<String, u32>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            fp_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Token auth ───────────────────────────────────────────────────────

    /// Produce a deterministic HMAC-SHA256 token for an email address.
    /// Used as the persistent localStorage session key after confirmation.
    pub fn sign_token(email: &str, secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
        mac.update(email.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Constant-time comparison against the expected token for an email.
    pub fn verify_token(email: &str, candidate: &str, secret: &[u8]) -> bool {
        let expected = Self::sign_token(email, secret);
        if expected.len() != candidate.len() {
            return false;
        }
        expected
            .bytes()
            .zip(candidate.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    /// One-time confirmation token: HMAC(email + "|" + session_id, secret).
    /// Ties the confirm link to a specific session so it can't be replayed.
    pub fn sign_confirm_token(email: &str, session_id: &str, secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
        mac.update(email.as_bytes());
        mac.update(b"|");
        mac.update(session_id.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn verify_confirm_token(email: &str, session_id: &str, candidate: &str, secret: &[u8]) -> bool {
        let expected = Self::sign_confirm_token(email, session_id, secret);
        if expected.len() != candidate.len() {
            return false;
        }
        expected
            .bytes()
            .zip(candidate.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    /// A token is 64 lowercase hex characters (SHA-256 output size).
    pub fn looks_like_token(s: &str) -> bool {
        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    // ── Fingerprint helpers ───────────────────────────────────────────────

    /// SHA-256 of `ip|fingerprint`, used as the secondary rate-limit bucket.
    pub fn fp_bucket(ip: &str, fingerprint: &str) -> String {
        let mut h = Sha256::new();
        h.update(ip.as_bytes());
        h.update(b"|");
        h.update(fingerprint.as_bytes());
        hex::encode(h.finalize())
    }

    // ── Session operations ────────────────────────────────────────────────

    /// Returns `true` and increments counters if the request is allowed.
    ///
    /// Two independent gates:
    /// 1. If the session is already unlocked → always allowed.
    /// 2. Otherwise: both the UUID bucket AND the fp bucket must be under
    ///    `FREE_MESSAGES`. Whichever is exhausted first blocks the request.
    pub async fn check_and_increment(
        &self,
        session_key: &str,
        fp_key: Option<&str>,
    ) -> bool {
        // Fast path: session already unlocked
        {
            let r = self.sessions.read().await;
            if r.get(session_key).map(|s| s.unlocked).unwrap_or(false) {
                drop(r);
                let mut w = self.sessions.write().await;
                if let Some(s) = w.get_mut(session_key) {
                    s.message_count += 1;
                }
                return true;
            }
        }

        // Check fingerprint bucket first (stronger anti-abuse signal)
        if let Some(fpk) = fp_key {
            let fp_count = {
                let r = self.fp_limits.read().await;
                *r.get(fpk).unwrap_or(&0)
            };
            if fp_count >= FREE_MESSAGES {
                return false;
            }
            // Both buckets under limit — allow and increment both
            {
                let mut w = self.sessions.write().await;
                let s = w.entry(session_key.to_string()).or_default();
                s.message_count += 1;
            }
            {
                let mut w = self.fp_limits.write().await;
                *w.entry(fpk.to_string()).or_default() += 1;
            }
            return true;
        }

        // No fingerprint — fall back to UUID-only gate
        let mut w = self.sessions.write().await;
        let s = w.entry(session_key.to_string()).or_default();
        if s.message_count < FREE_MESSAGES {
            s.message_count += 1;
            true
        } else {
            false
        }
    }

    /// Mark a session as permanently unlocked (email captured).
    /// Also pre-creates an unlocked session for the signed `token` so the
    /// token can serve as a session key on subsequent visits.
    pub async fn unlock_with_email(
        &self,
        session_key: &str,
        email: String,
        token: &str,
    ) -> bool {
        {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            if s.unlocked {
                return false;
            }
            s.unlocked = true;
            s.email = Some(email.clone());

            // Pre-create unlocked session for the token key
            let token_session = w.entry(token.to_string()).or_default();
            token_session.unlocked = true;
            token_session.email = Some(email.clone());
        }
        tracing::info!("Lead captured — email: {email} session: {session_key}");
        true
    }

    pub async fn message_count(&self, key: &str) -> u32 {
        self.sessions
            .read()
            .await
            .get(key)
            .map(|s| s.message_count)
            .unwrap_or(0)
    }

    pub async fn is_unlocked(&self, key: &str) -> bool {
        self.sessions
            .read()
            .await
            .get(key)
            .map(|s| s.unlocked)
            .unwrap_or(false)
    }
}
