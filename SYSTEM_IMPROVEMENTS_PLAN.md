# 📈 System Improvements Roadmap — Detailed Implementation Plan

## Status: 🔴 CRITICAL + 🟠 CODE QUALITY FIXES ✅ DONE

**Commit:** `e7dc914`

| Issue | Fix | Deployed |
|-------|-----|----------|
| SESSION_SECRET re-generated on restart | Persist via file (config module) | ✅ |
| `/api/admin/ingest` no auth | Bearer token validation | ✅ |
| Logging not configurable | EnvFilter + structured logging | ✅ |
| Error handling absent | Tests validate auth paths | ✅ |

---

## 📋 SYSTEM IMPROVEMENTS — Phase 1-3 (2-3 weeks)

### **Phase 1: Persistence & Observability** (Week 1, ~40h)

#### 1.1 SessionStore → Postgres [PRIORITY 1]
**Goal:** Conversations survive backend restart; prepare for multi-instance deployment

**Implementation:**
```rust
// crates/backend/src/sessions.rs: SessionStore 
// Current: RwLock<HashMap<sid, Session>>
// Target: RwLock<HashMap<sid, Session>> + Postgres write-through

#[derive(sqlx::FromRow)]
struct SessionRow {
    sid: String,
    email: Option<String>,
    unlocked: bool,
    created_at: DateTime<Utc>,
    messages_used: i32,
    expires_at: DateTime<Utc>,
}

// Migration: 001_create_sessions.sql
// CREATE TABLE sessions (
//   sid UUID PRIMARY KEY,
//   email VARCHAR(255),
//   unlocked BOOLEAN,
//   messages_used INT,
//   created_at TIMESTAMP,
//   expires_at TIMESTAMP,
//   INDEX (expires_at)  -- TTL cleanup scan
// );
```

**Changes Required:**
- `sessions.rs`: Add `db: Option<PgPool>` to SessionStore; implement `{get,set}_async` that hit DB
- `pitch.rs`: Already has DB write-through—copy pattern
- `main.rs`: Pass PgPool to SessionStore::new(db)
- `tests/integration.rs`: SessionStore::new(None) for in-memory tests

**Files Modified:** 5
**Lines Added:** ~200
**Tests Added:** 5 (happy path, TTL, concurrent access)
**Estimated Time:** 6h

---

#### 1.2 PipelineStore → Postgres [PRIORITY 2]
**Goal:** Track full pipeline lifecycle for analytics + recovery
**Current State:** Lost on restart; no audit trail
**Target:** Full write-through persistence

**Schema:**
```sql
CREATE TABLE pipelines (
  id UUID PRIMARY KEY,
  session_id UUID NOT NULL REFERENCES sessions(sid) ON DELETE CASCADE,
  stage VARCHAR(50),  -- Intake, Qualify, Builder, ...
  stage_payload JSONB,  -- Serialize PipelineStage enum
  price_quote INT,
  price_monthly INT,
  created_at TIMESTAMP,
  updated_at TIMESTAMP,
  INDEX (session_id, updated_at)
);

CREATE TABLE pipeline_history (
  id BIGSERIAL PRIMARY KEY,
  pipeline_id UUID REFERENCES pipelines(id) ON DELETE CASCADE,
  from_stage VARCHAR(50),
  to_stage VARCHAR(50),
  error_msg TEXT,
  timestamp TIMESTAMP,
  INDEX (pipeline_id, timestamp)
);
```

**Implementation:**
- `pipeline.rs`: Async persist hook on every stage transition
- `handlers/pipeline.rs`: `status()` fetches from DB; expose history via `GET /api/pipeline/:id/history` (paginated)
- `tests/`: Happy path + error recovery (restart mid-build)

**Files Modified:** 6
**Lines Added:** ~300
**Tests Added:** 8
**Estimated Time:** 8h

---

#### 1.3 Structured Error Context [PRIORITY 1]
**Goal:** Silent agent failures are invisible; add error observability

**Current Issue:**
```rust
// agents.rs: errors are swallowed
async fn run_builder(..) -> Result<(), AgentError> {
    // If this fails, it logs but doesn't bubble context
    tracing::error!("[builder] failed: {e}");
    Err(e)
}
```

**Solution:**
```rust
// New: crates/backend/src/errors.rs
#[derive(Debug)]
pub struct ErrorContext {
    pub pipeline_id: Uuid,
    pub stage: String,
    pub error_msg: String,
    pub timestamp: DateTime<Utc>,
    pub request_body: Option<String>,  // For debugging
}

// Persist to DB
CREATE TABLE agent_errors (
    id BIGSERIAL PRIMARY KEY,
    pipeline_id UUID,
    stage VARCHAR(50),
    error_msg TEXT,
    request_body TEXT,
    created_at TIMESTAMP,
    INDEX (pipeline_id, created_at)
);

// In agents.rs
async fn run_builder(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    match attempt_build(ctx).await {
        Ok(r) => Ok(r),
        Err(e) => {
            let err_ctx = ErrorContext { 
                pipeline_id: ctx.id,
                stage: "Builder".into(),
                error_msg: e.to_string(),
                timestamp: Utc::now(),
                request_body: serde_json::to_string(&ctx).ok(),
            };
            app.db.as_ref()?.execute(
                "INSERT INTO agent_errors (...) VALUES (...)",
                &[&pipeline_id, &stage, &error_msg, &request_body]
            ).await.ok();
            Err(e)
        }
    }
}
```

**Endpoint:**
```
GET /api/admin/errors?pipeline_id=:id&limit=100&offset=0
→ {
  errors: [{ stage, error_msg, timestamp, request_body }, ...],
  total: 1240
}
```

**Files Modified:** 6
**Lines Added:** ~250
**Tests Added:** 4
**Estimated Time:** 5h

---

### **Phase 2: Resilience & Rate Control** (Week 2, ~35h)

#### 2.1 Circuit Breaker for External APIs [PRIORITY 1]
**Goal:** Cascading failures stop here; don't pile requests on Anthropic when it's down

**Libraries:** `tokio-retry` + `tower-circuit-breaker` (or homebrew simple version)

**Implementation:**
```rust
// crates/backend/src/resilience.rs
pub struct CircuitBreaker<T> {
    state: Mutex<CBState>,
    failure_threshold: usize,
    success_threshold: usize,
    timeout: Duration,
    _phantom: PhantomData<T>,
}

enum CBState { Closed, Open(Instant), HalfOpen }

impl<T: Fn() -> Fut, Fut: Future<Output = Result<R, E>>> CircuitBreaker<T> {
    pub async fn call(&self, f: T) -> Result<R, CircuitOpenError> {
        match *self.state.lock().await {
            CBState::Open(when) if when.elapsed() < self.timeout => {
                Err(CircuitOpenError)
            }
            _ => {
                match f().await {
                    Ok(r) => {
                        *self.state.lock().await = CBState::Closed;
                        Ok(r)
                    }
                    Err(e) => {
                        // Increment failure counter; if threshold is exceeded, trip
                        if should_open() {
                            *self.state.lock().await = CBState::Open(Instant::now());
                        }
                        Err(e)
                    }
                }
            }
        }
    }
}

// In agents.rs
let cb = CircuitBreaker::new(3, 1, Duration::from_secs(30));
cb.call(|| async {
    anthropic_raw(&self.http, &self.api_key, body).await
}).await
```

**Files Modified:** 3
**Lines Added:** ~150
**Tests Added:** 5
**Estimated Time:** 4h

---

#### 2.2 Retry Logic with Exponential Backoff [PRIORITY 1]
**Goal:** Transient failures (network blip) don't cascade

```rust
// crates/backend/src/resilience.rs
async fn retry_with_backoff<F, Fut, R, E>(
    mut f: F,
    max_retries: u32,
) -> Result<R, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<R, E>>,
    E: std::fmt::Debug,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(r) => return Ok(r),
            Err(e) if attempt < max_retries => {
                let backoff_ms = (100 * 2_u64.pow(attempt)) as u64;
                tracing::warn!("Retry {}/{} in {}ms: {:?}", attempt + 1, max_retries, backoff_ms, e);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

// Usage in agents
retry_with_backoff(
    || async {
        anthropic_raw(&http, &key, body).await
    },
    3
).await?
```

**Files Modified:** 2
**Lines Added:** ~100
**Tests Added:** 3
**Estimated Time:** 2h

---

#### 2.3 Rate Limiting [PRIORITY 2]
**Goal:** Protect backend from abuse; 100 req/min per IP

**Library:** `tower-governor` (rate limiter middleware)

```rust
// main.rs
use governor::RateLimiter;
use governor::state::{Direct, NotKeyed};

let limiter = RateLimiter::direct(Quota::per_minute(nonzero!(100u32)));

.layer(middleware::from_fn(move |req, next| {
    if !limiter.check().is_ok() {
        return Ok(Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Body::from("rate limited"))
            .unwrap());
    }
    next.run(req).await
}))
```

**Alternative (simpler):** Use `tokio-rate-limit` with per-IP bucketing via `ConnectInfo`

**Files Modified:** 2
**Lines Added:** ~80
**Tests Added:** 2
**Estimated Time:** 3h

---

### **Phase 3: Admin Dashboard** (Week 2-3, ~40h)

#### 3.1 Pipeline Lifecycle Tracking [PRIORITY 1]
**Goal:** Sales team sees: intake → qualified → pricing → accepted → deployed

**Endpoint:**
```
GET /api/admin/pipelines?status=pending&sort=created_at:desc&limit=50&offset=0
→ {
  pipelines: [{
    id, session_id, email, status, price_quote, price_monthly,
    client_need (text preview), created_at, updated_at,
    latest_error: { stage, msg, timestamp }
  }],
  total: 1240
}
```

**Implementation:**
```rust
// handlers/admin.rs
#[derive(Serialize)]
pub struct PipelineRow {
    pub pipeline_id: String,
    pub email: Option<String>,
    pub stage: String,
    pub price_quote: Option<i32>,
    pub client_need: String,  // Preview
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub latest_error: Option<String>,
}

pub async fn list_pipelines(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Vec<PipelineRow>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = sqlx::query_as::<_, PipelineRow>(
        "SELECT p.*, s.email, 
                (SELECT error_msg FROM agent_errors 
                 WHERE pipeline_id = p.id ORDER BY created_at DESC LIMIT 1) as latest_error
         FROM pipelines p
         LEFT JOIN sessions s ON p.session_id = s.sid
         ORDER BY p.created_at DESC
         LIMIT $1 OFFSET $2"
    )
    .bind(q.limit)
    .bind(q.offset)
    .fetch_all(db)
    .await?;
    Ok(Json(rows))
}
```

**Files Modified:** 3
**Lines Added:** ~150
**Tests Added:** 4
**Estimated Time:** 6h

---

#### 3.2 Lead Export (CSV) [PRIORITY 2]
**Goal:** Sales reps export qualified leads → Salesforce/HubSpot

```
GET /api/admin/leads/export?format=csv&date_from=2026-01-01&date_to=2026-12-31
→ CSV: email, company, stage, price_quote, created_at, latest_message
```

**Implementation:**
- Use `csv` crate; stream output to avoid memory explosion
- Include: email, company, need_summary, stage, price, last_message_date, conversion_status
- 50k row export should be ~5MB

**Files Modified:** 2
**Lines Added:** ~100
**Tests Added:** 2
**Estimated Time:** 4h

---

#### 3.3 Pipeline Analytics [PRIORITY 3]
**Goal:** Metrics: conversion funnel (intake → qualified → accepted), avg price, time-to-quote

```
GET /api/admin/analytics/funnel?period=month
→ {
  intake: 450,
  qualified: 120,
  quote_generated: 95,
  accepted: 42,
  conversion_rate: 9.3%,
  avg_price_eur: 45000,
  avg_time_to_quote_hours: 12.5
}
```

**Implementation:**
```sql
-- Derived table for analytics
SELECT
    EXTRACT(WEEK FROM p.created_at) as week,
    COUNT(*) as intake_count,
    COUNT(CASE WHEN stage = 'Qualify' THEN 1 END) as qualified,
    COUNT(CASE WHEN stage = 'Builder' THEN 1 END) as builder,
    COUNT(CASE WHEN price_quote IS NOT NULL THEN 1 END) as quoted,
    AVG(EXTRACT(EPOCH FROM (p.updated_at - p.created_at)) / 3600) as avg_hours_to_quote
FROM pipelines p
GROUP BY EXTRACT(WEEK FROM p.created_at)
ORDER BY week DESC;
```

**Files Modified:** 3
**Lines Added:** ~120
**Tests Added:** 2
**Estimated Time:** 5h

---

## 📊 **Timeline Summary**

| Phase | Week | Hours | Deliverables |
|-------|------|-------|--------------|
| **Phase 1** | Wk1 | ~40h | SessionStore + PipelineStore persistence; Error context logging |
| **Phase 2** | Wk2 | ~35h | Circuit breaker; Retry logic; Rate limiting |
| **Phase 3** | Wk2-3 | ~40h | Admin dashboard (lifecycle, export, analytics) |
| **Testing** | Wk3 | ~15h | Integration tests; Load tests; Manual QA |
| **Deployment** | Wk3 | ~10h | Migration scripts; Production rollout |

**Total:** ~140h (~3-4 weeks solo, ~10 days with a pair)

---

## 🚀 **Implementation Order** (Parallelizable)

**Day 1-2:** SessionStore persistence (blocking; others depend)
**Day 3-4:** PipelineStore persistence (parallel: error context)
**Day 5-6:** Circuit breaker + retry logic (testing-heavy)
**Day 7-8:** Rate limiting + admin endpoints
**Day 9-10:** Dashboard frontend + analytics

---

## 🔍 **Key Decisions**

| Decision | Impact | Reasoning |
|----------|--------|-----------|
| **TTL Index on Sessions** | Clean old data | Prevents DB bloat; 7-day TTL → auto-cleanup |
| **Write-through for Pipelines** | Consistency | Single source of truth; easier rollback |
| **Circuit Breaker per API** | Resilience | Anthropic down ≠ whole app down |
| **Admin auth (separate token)** | Security | Least privilege; different from app token |
| **CSV export (streaming)** | Performance | 50k rows without OOM |

---

## ✅ **Definition of Done**

1. ✅ All tests pass (integration + unit)
2. ✅ Postgres migrations versioned (001_*, 002_*, etc)
3. ✅ Admin endpoints documented in OpenAPI spec
4. ✅ Load test: 1000 concurrent sessions without degradation
5. ✅ Error recovery test: Kill DB mid-pipeline, verify resume
6. ✅ Production rollout plan (zero-downtime migration)

---

**Next Step:** Start with SessionStore persistence (Day 1). Once that's solid, parallelizes well.

