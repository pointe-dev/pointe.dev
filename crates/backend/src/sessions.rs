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

    pub async fn get_email(&self, key: &str) -> Option<String> {
        self.sessions.read().await.get(key).and_then(|s| s.email.clone())
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

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no network, no database
// Covers  : token signing/verification, fingerprint hashing, rate-limit gating,
//           session unlock semantics, message counting
// Does NOT cover: Postgres persistence, HTTP handlers, concurrent races across
//                 multiple Tokio tasks
#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-for-unit-tests-only";

    // ── sign_token / verify_token ──────────────────────────────────────────

    #[test]
    fn sign_token_is_64_hex_chars() {
        let tok = SessionStore::sign_token("alice@example.com", SECRET);
        assert_eq!(tok.len(), 64);
        assert!(tok.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_token_same_inputs_same_output() {
        let a = SessionStore::sign_token("bob@example.com", SECRET);
        let b = SessionStore::sign_token("bob@example.com", SECRET);
        assert_eq!(a, b, "HMAC must be deterministic");
    }

    #[test]
    fn sign_token_different_emails_differ() {
        let a = SessionStore::sign_token("alice@example.com", SECRET);
        let b = SessionStore::sign_token("bob@example.com", SECRET);
        assert_ne!(a, b);
    }

    #[test]
    fn verify_token_correct_returns_true() {
        let tok = SessionStore::sign_token("user@example.com", SECRET);
        assert!(SessionStore::verify_token("user@example.com", &tok, SECRET));
    }

    #[test]
    fn verify_token_wrong_secret_returns_false() {
        let tok = SessionStore::sign_token("user@example.com", SECRET);
        assert!(!SessionStore::verify_token("user@example.com", &tok, b"wrong-secret"));
    }

    #[test]
    fn verify_token_wrong_email_returns_false() {
        let tok = SessionStore::sign_token("alice@example.com", SECRET);
        assert!(!SessionStore::verify_token("eve@example.com", &tok, SECRET));
    }

    #[test]
    fn verify_token_tampered_candidate_returns_false() {
        let mut tok = SessionStore::sign_token("user@example.com", SECRET);
        tok.replace_range(0..1, "f");
        // flip first char to something else if it was already 'f'
        if tok.starts_with('f') {
            tok.replace_range(0..1, "0");
        }
        assert!(!SessionStore::verify_token("user@example.com", &tok, SECRET));
    }

    // ── sign_confirm_token / verify_confirm_token ──────────────────────────

    #[test]
    fn confirm_token_binds_to_session() {
        let tok_a = SessionStore::sign_confirm_token("u@x.com", "session-1", SECRET);
        let tok_b = SessionStore::sign_confirm_token("u@x.com", "session-2", SECRET);
        assert_ne!(tok_a, tok_b, "different sessions → different tokens");
    }

    #[test]
    fn verify_confirm_token_correct_returns_true() {
        let tok = SessionStore::sign_confirm_token("u@x.com", "sess", SECRET);
        assert!(SessionStore::verify_confirm_token("u@x.com", "sess", &tok, SECRET));
    }

    #[test]
    fn verify_confirm_token_wrong_session_returns_false() {
        let tok = SessionStore::sign_confirm_token("u@x.com", "sess-A", SECRET);
        assert!(!SessionStore::verify_confirm_token("u@x.com", "sess-B", &tok, SECRET));
    }

    // ── looks_like_token ───────────────────────────────────────────────────

    #[test]
    fn looks_like_token_accepts_64_hex() {
        let tok = SessionStore::sign_token("x@y.com", SECRET);
        assert!(SessionStore::looks_like_token(&tok));
    }

    #[test]
    fn looks_like_token_rejects_uuid() {
        assert!(!SessionStore::looks_like_token("123e4567-e89b-12d3-a456-426614174000"));
    }

    #[test]
    fn looks_like_token_rejects_short_string() {
        assert!(!SessionStore::looks_like_token("abc123"));
    }

    #[test]
    fn looks_like_token_accepts_uppercase_hex() {
        // is_ascii_hexdigit() accepts A-F as well as a-f; the doc says
        // "lowercase" but the implementation is case-insensitive.
        let upper = "A".repeat(64);
        assert!(SessionStore::looks_like_token(&upper));
    }

    #[test]
    fn looks_like_token_rejects_non_hex_chars() {
        let bad = "G".repeat(64); // 'G' is not a hex digit
        assert!(!SessionStore::looks_like_token(&bad));
    }

    // ── fp_bucket ──────────────────────────────────────────────────────────

    #[test]
    fn fp_bucket_is_64_hex() {
        let b = SessionStore::fp_bucket("127.0.0.1", "abc123");
        assert_eq!(b.len(), 64);
        assert!(b.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn fp_bucket_same_inputs_stable() {
        assert_eq!(
            SessionStore::fp_bucket("10.0.0.1", "fp"),
            SessionStore::fp_bucket("10.0.0.1", "fp"),
        );
    }

    #[test]
    fn fp_bucket_different_ip_differs() {
        assert_ne!(
            SessionStore::fp_bucket("1.2.3.4", "fp"),
            SessionStore::fp_bucket("5.6.7.8", "fp"),
        );
    }

    // ── check_and_increment (UUID-only gate) ───────────────────────────────

    #[tokio::test]
    async fn free_messages_gate_uuid_only() {
        let store = SessionStore::new();
        for i in 0..FREE_MESSAGES {
            assert!(
                store.check_and_increment("sess-uuid", None).await,
                "message {} should be allowed", i + 1
            );
        }
        // 6th message must be blocked
        assert!(!store.check_and_increment("sess-uuid", None).await);
    }

    #[tokio::test]
    async fn message_count_increments_correctly() {
        let store = SessionStore::new();
        store.check_and_increment("s", None).await;
        store.check_and_increment("s", None).await;
        assert_eq!(store.message_count("s").await, 2);
    }

    #[tokio::test]
    async fn unknown_session_has_zero_count() {
        let store = SessionStore::new();
        assert_eq!(store.message_count("nobody").await, 0);
    }

    // ── check_and_increment (fingerprint gate) ─────────────────────────────

    #[tokio::test]
    async fn fp_gate_blocks_when_fp_exhausted() {
        let store = SessionStore::new();
        let fp = "fp-bucket-key";
        // exhaust with session-A
        for _ in 0..FREE_MESSAGES {
            store.check_and_increment("sess-A", Some(fp)).await;
        }
        // session-B with same fp must be blocked
        assert!(!store.check_and_increment("sess-B-fresh", Some(fp)).await);
    }

    #[tokio::test]
    async fn unlocked_session_bypasses_limit() {
        let store = SessionStore::new();
        let tok = SessionStore::sign_token("vip@example.com", SECRET);
        // Unlock the session first
        store.unlock_with_email("sess-vip", "vip@example.com".to_string(), &tok).await;
        // Should allow messages beyond the free tier
        for _ in 0..(FREE_MESSAGES + 10) {
            assert!(store.check_and_increment("sess-vip", None).await);
        }
    }

    // ── unlock_with_email ──────────────────────────────────────────────────

    #[tokio::test]
    async fn unlock_with_email_creates_token_session() {
        let store = SessionStore::new();
        let tok = SessionStore::sign_token("new@example.com", SECRET);
        let did_unlock = store.unlock_with_email("sess-x", "new@example.com".to_string(), &tok).await;
        assert!(did_unlock);
        assert!(store.is_unlocked("sess-x").await);
        // Token-keyed session should also be unlocked
        assert!(store.is_unlocked(&tok).await);
    }

    #[tokio::test]
    async fn unlock_with_email_idempotent() {
        let store = SessionStore::new();
        let tok = SessionStore::sign_token("once@example.com", SECRET);
        assert!(store.unlock_with_email("sess-once", "once@example.com".to_string(), &tok).await);
        // Second call on already-unlocked session returns false
        assert!(!store.unlock_with_email("sess-once", "once@example.com".to_string(), &tok).await);
    }

    #[tokio::test]
    async fn get_email_returns_stored_email() {
        let store = SessionStore::new();
        let tok = SessionStore::sign_token("stored@example.com", SECRET);
        store.unlock_with_email("sess-email", "stored@example.com".to_string(), &tok).await;
        assert_eq!(
            store.get_email("sess-email").await,
            Some("stored@example.com".to_string())
        );
    }

    #[tokio::test]
    async fn is_unlocked_false_for_new_session() {
        let store = SessionStore::new();
        assert!(!store.is_unlocked("brand-new-session").await);
    }
}
