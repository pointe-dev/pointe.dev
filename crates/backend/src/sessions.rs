use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};

type HmacSha256 = Hmac<Sha256>;

// ── Credit pricing (all amounts in cents; adjustable in one line each) ───────────
/// Free credits granted once, when the visitor captures their email.
pub const SIGNUP_CREDITS_CENTS: i64 = 500; // 5 $
/// Cost charged per chat message to the agent.
pub const MESSAGE_COST_CENTS: i64 = 10; // 0,10 $
/// Pre-payment pipeline step costs (heavier than a chat message). The build/deploy
/// steps run *after* payment, so they don't consume free chat credits — no BUILD cost.
pub const RESEARCH_COST_CENTS: i64 = 50; // 0,50 $
pub const DESIGN_COST_CENTS: i64 = 50; // 0,50 $
pub const PITCH_COST_CENTS: i64 = 50; // 0,50 $
/// Default top-up amount offered when credits run out.
pub const TOPUP_DEFAULT_CENTS: i64 = 1000; // 10 $

/// Monthly chat-credit allocation granted by a *project subscription*, by tier.
/// These reset to the allocation each month (non-cumulative). PLACEHOLDER values —
/// the owner sets the real per-tier numbers here (one line each).
pub const MONTHLY_GIFT_INSTANT_CENTS: i64 = 0; // TODO(owner): set Instant-tier monthly chat credits
pub const MONTHLY_GIFT_ASSISTED_CENTS: i64 = 0; // TODO(owner): set Assisted-tier monthly chat credits
pub const MONTHLY_GIFT_MANAGED_CENTS: i64 = 0; // TODO(owner): set Managed-tier monthly chat credits

/// Resolve a tier slug ("instant"/"assisted"/"managed") to its monthly allocation.
/// Used when the pitch checkout is converted to a real Stripe subscription (the
/// `mode=subscription` wiring is the next step); kept ready for that call site.
#[allow(dead_code)]
pub fn monthly_gift_for_tier(tier: &str) -> i64 {
    match tier.to_lowercase().as_str() {
        "instant" => MONTHLY_GIFT_INSTANT_CENTS,
        "assisted" => MONTHLY_GIFT_ASSISTED_CENTS,
        "managed" => MONTHLY_GIFT_MANAGED_CENTS,
        _ => 0,
    }
}

/// Current calendar month as "YYYY-MM" (UTC), used to detect a new gift period.
fn current_period() -> String {
    chrono::Utc::now().format("%Y-%m").to_string()
}

#[derive(Default, Clone)]
pub struct Session {
    /// Lifetime message counter — kept for stats only, no longer a gate.
    pub message_count: u32,
    /// True once the visitor has *captured* their email (chat enabled, 5 $ granted).
    pub unlocked: bool,
    /// True only after the double-opt-in confirmation link is clicked. Required
    /// before payment, not before chatting.
    pub email_verified: bool,
    pub email: Option<String>,
    /// Free chat credits (cents). Reset monthly to the subscription allocation;
    /// consumed before purchased credits. Starts at SIGNUP_CREDITS_CENTS at capture.
    pub gift_credits_cents: i64,
    /// Purchased chat credits (cents). Never reset; consumed after gift credits.
    pub purchased_credits_cents: i64,
    /// Monthly gift allocation in cents (0 for a non-subscriber). Re-applied to
    /// gift_credits_cents at the start of each new period.
    pub monthly_gift_cents: i64,
    /// "YYYY-MM" of the last gift allocation, so we reset gift credits once per month.
    pub gift_period: Option<String>,
}

impl Session {
    /// Total spendable balance (gift + purchased), in cents.
    pub fn balance_cents(&self) -> i64 {
        self.gift_credits_cents + self.purchased_credits_cents
    }
}

/// In-memory store with optional Postgres write-through.
/// Memory is L1 (the hot path stays lock-only, no DB await under load).
/// DB is L2 (survives restarts). On boot the maps are hydrated from DB so the
/// existing in-memory gate logic stays correct without per-read DB lookups.
/// If DATABASE_URL is not set the store works purely in-memory.
#[derive(Clone)]
pub struct SessionStore {
    /// Primary sessions keyed by session_id (UUID or signed token).
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    /// Secondary rate limit keyed by SHA-256(ip|fingerprint).
    /// Prevents localStorage-clearing from resetting the free tier.
    fp_limits: Arc<RwLock<HashMap<String, u32>>>,
    db: Option<sqlx::PgPool>,
}

impl SessionStore {
    /// In-memory only (db = None). Used by tests and the no-DATABASE_URL path.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            fp_limits: Arc::new(RwLock::new(HashMap::new())),
            db: None,
        }
    }

    /// Construct with optional Postgres write-through, hydrating the in-memory
    /// maps from the `sessions` / `fp_limits` tables so restarts don't reset
    /// unlock state or the free-message gate.
    pub async fn with_db(db: Option<sqlx::PgPool>) -> Self {
        let store = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            fp_limits: Arc::new(RwLock::new(HashMap::new())),
            db,
        };
        store.hydrate().await;
        store
    }

    async fn hydrate(&self) {
        let Some(pool) = &self.db else { return };

        match sqlx::query_as::<_, (String, i32, bool, bool, Option<String>, i64, i64, i64, Option<String>)>(
            "SELECT session_key, message_count, unlocked, email_verified, email, \
                    gift_credits_cents, purchased_credits_cents, monthly_gift_cents, gift_period \
             FROM sessions",
        )
        .fetch_all(pool)
        .await
        {
            Ok(rows) => {
                let mut w = self.sessions.write().await;
                for (key, count, unlocked, email_verified, email,
                     gift, purchased, monthly_gift, gift_period) in rows {
                    w.insert(key, Session {
                        message_count: count.max(0) as u32,
                        unlocked,
                        email_verified,
                        email,
                        gift_credits_cents: gift,
                        purchased_credits_cents: purchased,
                        monthly_gift_cents: monthly_gift,
                        gift_period,
                    });
                }
                tracing::info!("[sessions] hydrated {} sessions from DB", w.len());
            }
            Err(e) => tracing::warn!("[sessions] hydrate sessions failed: {e}"),
        }

        match sqlx::query_as::<_, (String, i32)>(
            "SELECT fp_key, message_count FROM fp_limits",
        )
        .fetch_all(pool)
        .await
        {
            Ok(rows) => {
                let mut w = self.fp_limits.write().await;
                for (key, count) in rows {
                    w.insert(key, count.max(0) as u32);
                }
                tracing::info!("[sessions] hydrated {} fp buckets from DB", w.len());
            }
            Err(e) => tracing::warn!("[sessions] hydrate fp_limits failed: {e}"),
        }
    }

    // ── Write-through helpers ─────────────────────────────────────────────
    // Called after the in-memory mutation, with the lock already released, so
    // a slow/failed DB write never blocks or breaks the request path.

    async fn persist_session(&self, key: &str, s: &Session) {
        let Some(pool) = &self.db else { return };
        if let Err(e) = sqlx::query(
            "INSERT INTO sessions (session_key, message_count, unlocked, email_verified, email, \
                                   gift_credits_cents, purchased_credits_cents, monthly_gift_cents, \
                                   gift_period, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
             ON CONFLICT (session_key) DO UPDATE SET
                 message_count           = EXCLUDED.message_count,
                 unlocked                = EXCLUDED.unlocked,
                 email_verified          = EXCLUDED.email_verified,
                 email                   = EXCLUDED.email,
                 gift_credits_cents      = EXCLUDED.gift_credits_cents,
                 purchased_credits_cents = EXCLUDED.purchased_credits_cents,
                 monthly_gift_cents      = EXCLUDED.monthly_gift_cents,
                 gift_period             = EXCLUDED.gift_period,
                 updated_at              = NOW()",
        )
        .bind(key)
        .bind(s.message_count as i32)
        .bind(s.unlocked)
        .bind(s.email_verified)
        .bind(&s.email)
        .bind(s.gift_credits_cents)
        .bind(s.purchased_credits_cents)
        .bind(s.monthly_gift_cents)
        .bind(&s.gift_period)
        .execute(pool)
        .await
        {
            tracing::warn!("[sessions] DB write failed for session={key}: {e}");
        }
    }

    async fn persist_fp(&self, key: &str, count: u32) {
        let Some(pool) = &self.db else { return };
        if let Err(e) = sqlx::query(
            "INSERT INTO fp_limits (fp_key, message_count, updated_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (fp_key) DO UPDATE SET
                 message_count = EXCLUDED.message_count,
                 updated_at    = NOW()",
        )
        .bind(key)
        .bind(count as i32)
        .execute(pool)
        .await
        {
            tracing::warn!("[sessions] fp DB write failed for {key}: {e}");
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

    // ── Credit operations ─────────────────────────────────────────────────

    /// Apply a lazy monthly reset to gift credits in place: if this session has a
    /// monthly allocation and the period rolled over, reset gift credits to the
    /// allocation (non-cumulative). Returns true if anything changed. Caller holds
    /// the write lock.
    fn apply_monthly_reset(s: &mut Session) -> bool {
        if s.monthly_gift_cents <= 0 {
            return false;
        }
        let now = current_period();
        if s.gift_period.as_deref() != Some(now.as_str()) {
            s.gift_credits_cents = s.monthly_gift_cents;
            s.gift_period = Some(now);
            return true;
        }
        false
    }

    /// Grant the one-time signup credits when a visitor captures their email.
    /// Idempotent per session (only credits a session whose gift_period is unset
    /// and balance is zero). The fingerprint bucket blocks re-farming the free
    /// credits by clearing localStorage. Returns true if credits were granted.
    pub async fn grant_signup_credits(&self, session_key: &str, fp_key: Option<&str>) -> bool {
        // Anti-farm: a fingerprint that already claimed signup credits can't reclaim.
        if let Some(fpk) = fp_key {
            let already = { *self.fp_limits.read().await.get(fpk).unwrap_or(&0) > 0 };
            if already {
                return false;
            }
        }
        let snapshot = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            // Only grant once: a session that has already been credited has a
            // non-empty gift_period or a positive lifetime balance.
            if s.gift_period.is_some() || s.balance_cents() > 0 {
                return false;
            }
            s.gift_credits_cents = SIGNUP_CREDITS_CENTS;
            s.gift_period = Some(current_period());
            s.clone()
        };
        if let Some(fpk) = fp_key {
            let total = {
                let mut w = self.fp_limits.write().await;
                let c = w.entry(fpk.to_string()).or_default();
                *c += 1;
                *c
            };
            self.persist_fp(fpk, total).await;
        }
        self.persist_session(session_key, &snapshot).await;
        true
    }

    /// Charge `cost_cents` against the session, spending gift credits first then
    /// purchased credits. Applies the lazy monthly reset before charging. Returns
    /// true if the balance covered the cost (and was debited), false otherwise.
    pub async fn charge(&self, session_key: &str, cost_cents: i64) -> bool {
        let snapshot = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            Self::apply_monthly_reset(s);
            if s.balance_cents() < cost_cents {
                // Persist any reset that happened even when the charge fails.
                let snap = s.clone();
                drop(w);
                self.persist_session(session_key, &snap).await;
                return false;
            }
            // Spend gift first, then purchased.
            let from_gift = cost_cents.min(s.gift_credits_cents);
            s.gift_credits_cents -= from_gift;
            s.purchased_credits_cents -= cost_cents - from_gift;
            s.message_count += 1;
            s.clone()
        };
        self.persist_session(session_key, &snapshot).await;
        true
    }

    /// Add purchased (persistent) credits — called by the Stripe top-up webhook.
    pub async fn add_purchased_credits(&self, session_key: &str, cents: i64) {
        let snapshot = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            s.purchased_credits_cents += cents;
            s.clone()
        };
        self.persist_session(session_key, &snapshot).await;
    }

    /// Activate a project subscription's monthly chat-credit allocation. Sets the
    /// monthly amount and grants the first period immediately. Idempotent re-calls
    /// with the same amount in the same period are harmless.
    pub async fn set_monthly_gift(&self, session_key: &str, cents: i64) {
        let snapshot = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            s.monthly_gift_cents = cents;
            // Grant the allocation for the current period now.
            s.gift_credits_cents = cents;
            s.gift_period = Some(current_period());
            s.clone()
        };
        self.persist_session(session_key, &snapshot).await;
    }

    /// Spendable balance (gift + purchased), in cents.
    pub async fn balance_cents(&self, session_key: &str) -> i64 {
        let mut w = self.sessions.write().await;
        if let Some(s) = w.get_mut(session_key) {
            // Surface the post-reset balance so the UI is accurate at month rollover.
            if Self::apply_monthly_reset(s) {
                let snap = s.clone();
                drop(w);
                self.persist_session(session_key, &snap).await;
                return snap.balance_cents();
            }
            s.balance_cents()
        } else {
            0
        }
    }

    /// Mark the session's email as verified (double-opt-in link clicked).
    pub async fn mark_email_verified(&self, session_key: &str) {
        let snapshot = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            s.email_verified = true;
            s.clone()
        };
        self.persist_session(session_key, &snapshot).await;
    }

    /// Whether the session's email has been verified via the confirmation link.
    /// Consumed by the verify-before-payment gate (next wiring step).
    #[allow(dead_code)]
    pub async fn is_email_verified(&self, key: &str) -> bool {
        self.sessions.read().await.get(key).map(|s| s.email_verified).unwrap_or(false)
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
        let (session_snapshot, token_snapshot) = {
            let mut w = self.sessions.write().await;
            let s = w.entry(session_key.to_string()).or_default();
            if s.unlocked {
                return false;
            }
            s.unlocked = true;
            s.email = Some(email.clone());
            let session_snapshot = s.clone();

            // Pre-create unlocked session for the token key, mirroring the credit
            // state so the token can serve as the session key on later visits.
            let token_session = w.entry(token.to_string()).or_default();
            token_session.unlocked = true;
            token_session.email = Some(email.clone());
            token_session.email_verified = session_snapshot.email_verified;
            token_session.gift_credits_cents = session_snapshot.gift_credits_cents;
            token_session.purchased_credits_cents = session_snapshot.purchased_credits_cents;
            token_session.monthly_gift_cents = session_snapshot.monthly_gift_cents;
            token_session.gift_period = session_snapshot.gift_period.clone();
            (session_snapshot, token_session.clone())
        };
        self.persist_session(session_key, &session_snapshot).await;
        self.persist_session(token, &token_snapshot).await;
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

/// Creates the sessions / fp_limits tables if they don't exist.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            session_key   TEXT PRIMARY KEY,
            message_count INTEGER     NOT NULL DEFAULT 0,
            unlocked      BOOLEAN     NOT NULL DEFAULT FALSE,
            email         TEXT,
            updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // Credit-model columns (added 2026-06; idempotent for existing deployments).
    for ddl in [
        "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS email_verified BOOLEAN NOT NULL DEFAULT FALSE",
        "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS gift_credits_cents BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS purchased_credits_cents BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS monthly_gift_cents BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS gift_period TEXT",
    ] {
        sqlx::query(ddl).execute(pool).await?;
    }

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fp_limits (
            fp_key        TEXT PRIMARY KEY,
            message_count INTEGER     NOT NULL DEFAULT 0,
            updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    tracing::info!("[sessions] DB migration complete");
    Ok(())
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

    // ── credits: signup grant + charge gate ────────────────────────────────

    #[tokio::test]
    async fn signup_grants_five_dollars_once() {
        let store = SessionStore::new();
        assert!(store.grant_signup_credits("sess-c", None).await);
        assert_eq!(store.balance_cents("sess-c").await, SIGNUP_CREDITS_CENTS);
        // A second grant on the same session is a no-op (already credited).
        assert!(!store.grant_signup_credits("sess-c", None).await);
        assert_eq!(store.balance_cents("sess-c").await, SIGNUP_CREDITS_CENTS);
    }

    #[tokio::test]
    async fn signup_not_regranted_for_same_fingerprint() {
        let store = SessionStore::new();
        let fp = "fp-farm";
        assert!(store.grant_signup_credits("sess-1", Some(fp)).await);
        // Clearing localStorage = new session id, same fingerprint → blocked.
        assert!(!store.grant_signup_credits("sess-2-fresh", Some(fp)).await);
        assert_eq!(store.balance_cents("sess-2-fresh").await, 0);
    }

    #[tokio::test]
    async fn charge_spends_gift_then_purchased() {
        let store = SessionStore::new();
        store.grant_signup_credits("s", None).await; // 500 gift
        store.add_purchased_credits("s", 100).await; // +100 purchased = 600 total
        // Spend 550: 500 from gift, 50 from purchased.
        assert!(store.charge("s", 550).await);
        assert_eq!(store.balance_cents("s").await, 50);
    }

    #[tokio::test]
    async fn charge_refused_when_insufficient() {
        let store = SessionStore::new();
        store.grant_signup_credits("s", None).await; // 500
        assert!(store.charge("s", 500).await); // exact spend → 0
        assert!(!store.charge("s", MESSAGE_COST_CENTS).await); // nothing left
        assert_eq!(store.balance_cents("s").await, 0);
    }

    #[tokio::test]
    async fn message_count_tracks_successful_charges() {
        let store = SessionStore::new();
        store.grant_signup_credits("s", None).await;
        store.charge("s", MESSAGE_COST_CENTS).await;
        store.charge("s", MESSAGE_COST_CENTS).await;
        assert_eq!(store.message_count("s").await, 2);
    }

    #[tokio::test]
    async fn unknown_session_has_zero_balance() {
        let store = SessionStore::new();
        assert_eq!(store.balance_cents("nobody").await, 0);
        assert_eq!(store.message_count("nobody").await, 0);
    }

    // ── credits: monthly subscription allocation ────────────────────────────

    #[tokio::test]
    async fn monthly_gift_resets_on_new_period() {
        let store = SessionStore::new();
        store.set_monthly_gift("sub", 2000).await; // allocation 20 $, current month
        assert_eq!(store.balance_cents("sub").await, 2000);
        // Spend half, then force a stale period to simulate month rollover.
        store.charge("sub", 1000).await;
        {
            let mut w = store.sessions.write().await;
            w.get_mut("sub").unwrap().gift_period = Some("2000-01".to_string());
        }
        // Next charge applies the reset first → back to full allocation, minus cost.
        assert!(store.charge("sub", 100).await);
        assert_eq!(store.balance_cents("sub").await, 2000 - 100);
    }

    #[tokio::test]
    async fn purchased_credits_never_reset_monthly() {
        let store = SessionStore::new();
        store.set_monthly_gift("sub", 1000).await;
        store.add_purchased_credits("sub", 700).await; // persistent
        // Force month rollover.
        {
            let mut w = store.sessions.write().await;
            w.get_mut("sub").unwrap().gift_period = Some("2000-01".to_string());
        }
        // Reset restores gift to 1000 but leaves purchased intact → 1700.
        assert_eq!(store.balance_cents("sub").await, 1700);
    }

    #[tokio::test]
    async fn email_verified_flag_toggles() {
        let store = SessionStore::new();
        assert!(!store.is_email_verified("s").await);
        store.mark_email_verified("s").await;
        assert!(store.is_email_verified("s").await);
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

    // ── Postgres write-through / hydration ─────────────────────────────────
    //
    // These require a real Postgres. They read TEST_DATABASE_URL and skip
    // (pass) when it is unset so CI — which has no DB service — stays green.
    // Run locally with: TEST_DATABASE_URL=postgres://… cargo test -p backend

    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("TEST_DATABASE_URL set but connection failed");
        run_migrations(&pool).await.expect("migrations should run");
        Some(pool)
    }

    #[tokio::test]
    async fn with_db_none_behaves_in_memory() {
        let store = SessionStore::with_db(None).await;
        store.grant_signup_credits("k", None).await;
        // Spend the whole 5 $ at the per-message rate, then the next charge fails.
        let msgs = (SIGNUP_CREDITS_CENTS / MESSAGE_COST_CENTS) as usize;
        for _ in 0..msgs {
            assert!(store.charge("k", MESSAGE_COST_CENTS).await);
        }
        assert!(!store.charge("k", MESSAGE_COST_CENTS).await);
    }

    #[tokio::test]
    async fn session_state_survives_restart() {
        let Some(pool) = test_pool().await else { return };
        let key = format!("hydrate-test-{}", uuid_like());
        // clean any prior run
        sqlx::query("DELETE FROM sessions WHERE session_key = $1")
            .bind(&key).execute(&pool).await.unwrap();

        let store = SessionStore::with_db(Some(pool.clone())).await;
        store.grant_signup_credits(&key, None).await;
        store.charge(&key, MESSAGE_COST_CENTS).await;
        store.charge(&key, MESSAGE_COST_CENTS).await;
        let expected = SIGNUP_CREDITS_CENTS - 2 * MESSAGE_COST_CENTS;

        // Simulate a restart: brand-new store, same pool — must hydrate.
        let restarted = SessionStore::with_db(Some(pool.clone())).await;
        assert_eq!(restarted.message_count(&key).await, 2);
        assert_eq!(restarted.balance_cents(&key).await, expected, "credits must persist");

        sqlx::query("DELETE FROM sessions WHERE session_key = $1")
            .bind(&key).execute(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn unlock_survives_restart() {
        let Some(pool) = test_pool().await else { return };
        let key = format!("unlock-test-{}", uuid_like());
        let tok = SessionStore::sign_token("restart@example.com", SECRET);
        sqlx::query("DELETE FROM sessions WHERE session_key = ANY($1)")
            .bind(vec![key.clone(), tok.clone()]).execute(&pool).await.unwrap();

        let store = SessionStore::with_db(Some(pool.clone())).await;
        store.unlock_with_email(&key, "restart@example.com".to_string(), &tok).await;

        let restarted = SessionStore::with_db(Some(pool.clone())).await;
        assert!(restarted.is_unlocked(&key).await, "unlock must persist across restart");
        assert!(restarted.is_unlocked(&tok).await, "token session must persist too");
        assert_eq!(restarted.get_email(&key).await, Some("restart@example.com".to_string()));

        sqlx::query("DELETE FROM sessions WHERE session_key = ANY($1)")
            .bind(vec![key, tok]).execute(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn fp_limit_survives_restart() {
        let Some(pool) = test_pool().await else { return };
        let fp = format!("fp-test-{}", uuid_like());
        sqlx::query("DELETE FROM fp_limits WHERE fp_key = $1")
            .bind(&fp).execute(&pool).await.unwrap();

        let store = SessionStore::with_db(Some(pool.clone())).await;
        // Claim signup credits once with this fingerprint.
        assert!(store.grant_signup_credits("sess-fp", Some(&fp)).await);

        // After restart, a fresh session sharing the fp must NOT get credits again.
        let restarted = SessionStore::with_db(Some(pool.clone())).await;
        assert!(!restarted.grant_signup_credits("fresh-sess", Some(&fp)).await);
        assert_eq!(restarted.balance_cents("fresh-sess").await, 0);

        sqlx::query("DELETE FROM fp_limits WHERE fp_key = $1")
            .bind(&fp).execute(&pool).await.unwrap();
    }

    // Cheap unique suffix so concurrent test runs don't collide on keys.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!("{nanos}")
    }
}
