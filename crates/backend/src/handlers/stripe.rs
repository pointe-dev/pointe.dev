use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::pipeline;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub pipeline_id: Uuid,
}

#[derive(Serialize)]
pub struct CheckoutResponse {
    pub checkout_url: String,
    pub session_id: String,
}

/// POST /api/stripe/checkout
/// Called by the frontend once the pipeline reaches AwaitingPayment.
/// Returns a Stripe Checkout URL to redirect the client to.
pub async fn create_checkout(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, String)> {
    let stripe = state.stripe.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Stripe not configured".to_string()))?;

    let (price_eur, price_monthly_eur, workflow_name) = {
        let guard = state.pipelines.0.read().await;
        let record = guard.get(&payload.pipeline_id)
            .ok_or((StatusCode::NOT_FOUND, "pipeline not found".to_string()))?;

        let price = record.ctx.price_quote
            .ok_or((StatusCode::CONFLICT, "pipeline has no price quote yet".to_string()))?;

        let monthly = record.ctx.price_monthly.unwrap_or(0);

        let name = record.ctx.workflow_json
            .as_ref()
            .and_then(|w| w["name"].as_str())
            .unwrap_or("Workflow d'automatisation")
            .to_string();

        (price, monthly, name)
    };

    let app_url = std::env::var("APP_URL")
        .unwrap_or_else(|_| "https://go.pointe.dev".to_string());

    let success_url = format!(
        "{app_url}/merci?pipeline={}",
        payload.pipeline_id
    );
    let cancel_url = format!("{app_url}/#chat");

    let session = stripe
        .create_checkout(payload.pipeline_id, price_eur, price_monthly_eur, &workflow_name, &success_url, &cancel_url)
        .await
        .map_err(|e| {
            tracing::error!("[stripe] checkout failed: {e}");
            (StatusCode::BAD_GATEWAY, e)
        })?;

    tracing::info!(
        "[stripe] checkout created session={} pipeline={} setup={price_eur}€ monthly={price_monthly_eur}€",
        session.id, payload.pipeline_id
    );

    Ok(Json(CheckoutResponse {
        checkout_url: session.url,
        session_id: session.id,
    }))
}

#[derive(Deserialize)]
pub struct TopupRequest {
    /// The chat session id to credit after payment.
    pub session_id: String,
    /// How many credit cents to buy. Defaults to TOPUP_DEFAULT_CENTS if absent/0.
    #[serde(default)]
    pub credit_cents: i64,
}

/// POST /api/credits/topup
/// Returns a Stripe Checkout URL to buy chat credits. The paid webhook adds the
/// purchased credits to the session (kind=topup).
pub async fn create_topup(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TopupRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, String)> {
    let stripe = state.stripe.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Stripe not configured".to_string()))?;

    let credit_cents = if payload.credit_cents > 0 {
        payload.credit_cents
    } else {
        crate::sessions::TOPUP_DEFAULT_CENTS
    };
    // Price == credits (no markup for now); both are in cents.
    let price_cents = credit_cents;

    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "https://go.pointe.dev".to_string());
    let success_url = format!("{app_url}/#chat");
    let cancel_url = format!("{app_url}/#chat");

    let session = stripe
        .create_credit_topup_checkout(&payload.session_id, credit_cents, price_cents, &success_url, &cancel_url)
        .await
        .map_err(|e| {
            tracing::error!("[stripe] topup checkout failed: {e}");
            (StatusCode::BAD_GATEWAY, e)
        })?;

    tracing::info!("[stripe] topup checkout created session={} credit={credit_cents}c", session.id);
    Ok(Json(CheckoutResponse {
        checkout_url: session.url,
        session_id: session.id,
    }))
}

/// POST /api/stripe/webhook
/// Stripe sends events here. We only act on `checkout.session.completed`.
/// Must consume raw bytes (not Json) to verify the HMAC signature.
pub async fn webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let stripe = match &state.stripe {
        Some(s) => s,
        None => {
            tracing::warn!("[stripe] webhook received but Stripe not configured");
            return StatusCode::SERVICE_UNAVAILABLE;
        }
    };

    let sig_header = match headers.get("stripe-signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None => {
            tracing::warn!("[stripe] webhook missing Stripe-Signature header");
            return StatusCode::BAD_REQUEST;
        }
    };

    let event = match stripe.verify_webhook(&body, sig_header) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("[stripe] webhook verification failed: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    let event_type = event["type"].as_str().unwrap_or("unknown");
    let session = &event["data"]["object"];
    // Stripe Checkout payment_status: "paid" | "unpaid" | "no_payment_required".
    // For card (synchronous) it's already "paid" on completed; for async methods
    // (SEPA, Bancontact…) completed arrives "unpaid" and only async_payment_succeeded
    // later flips it to "paid".
    let payment_status = session["payment_status"].as_str().unwrap_or("");
    tracing::info!("[stripe] webhook event={event_type} payment_status={payment_status}");

    match event_type {
        // Synchronous (card) success — but only act once the money is actually captured.
        // Guarding on payment_status prevents resuming a pipeline for an async method
        // whose payment is still pending and could yet fail.
        "checkout.session.completed" => {
            if payment_status == "paid" {
                handle_paid_session(&state, session).await;
            } else {
                tracing::info!(
                    "[stripe] checkout.session.completed but payment_status={payment_status} \
                     — awaiting async_payment_succeeded before resuming"
                );
            }
        }
        // Async method (SEPA debit, etc.) cleared after the fact — this is the real
        // money-confirmed signal for those flows.
        "checkout.session.async_payment_succeeded" => {
            handle_paid_session(&state, session).await;
        }
        // Recurring subscription invoice paid → re-apply the monthly chat-credit
        // allocation (the gift pocket resets each period; purchased credits persist).
        "invoice.paid" | "invoice.payment_succeeded" => {
            let md = &event["data"]["object"]["lines"]["data"][0]["metadata"];
            let session_key = md["session_id"].as_str()
                .or_else(|| event["data"]["object"]["subscription_details"]["metadata"]["session_id"].as_str())
                .unwrap_or("");
            let gift = md["monthly_gift_cents"].as_str().and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            if !session_key.is_empty() && gift > 0 {
                state.sessions.set_monthly_gift(session_key, gift).await;
                tracing::info!("[stripe] invoice.paid — monthly gift {gift}c re-applied to {session_key}");
            }
        }
        // Async payment bounced — leave the pipeline parked at AwaitingPayment so the
        // client can retry; never advance delivery on a failed payment.
        "checkout.session.async_payment_failed" => {
            let pid = session["metadata"]["pipeline_id"].as_str().unwrap_or("");
            tracing::warn!("[stripe] async payment FAILED for pipeline {pid} — pipeline left at AwaitingPayment");
        }
        // Abandoned session — nothing to do; pipeline stays parked.
        "checkout.session.expired" => {
            let pid = session["metadata"]["pipeline_id"].as_str().unwrap_or("");
            tracing::info!("[stripe] checkout session expired for pipeline {pid}");
        }
        _ => {}
    }

    // Always return 200 so Stripe doesn't retry
    StatusCode::OK
}

/// Route a paid Checkout Session by `metadata.kind`:
/// - `topup`        → add purchased chat credits to the session.
/// - `project_sub`  → activate the project's monthly chat-credit allocation, then
///   resume the parked pipeline (setup + recurring just paid).
/// - default/legacy → resume the parked pipeline (pitch payment).
async fn handle_paid_session(state: &Arc<AppState>, session: &serde_json::Value) {
    let kind = session["metadata"]["kind"].as_str().unwrap_or("");
    match kind {
        "topup" => {
            let session_key = session["metadata"]["session_id"].as_str().unwrap_or("");
            let cents = session["metadata"]["credit_cents"].as_str()
                .and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            if !session_key.is_empty() && cents > 0 {
                state.sessions.add_purchased_credits(session_key, cents).await;
                tracing::info!("[stripe] top-up — {cents}c purchased credits added to {session_key}");
            } else {
                tracing::warn!("[stripe] top-up webhook missing session_id/credit_cents metadata");
            }
        }
        "project_sub" => {
            let session_key = session["metadata"]["session_id"].as_str().unwrap_or("");
            let gift = session["metadata"]["monthly_gift_cents"].as_str()
                .and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            if !session_key.is_empty() && gift > 0 {
                state.sessions.set_monthly_gift(session_key, gift).await;
                tracing::info!("[stripe] project_sub — monthly gift {gift}c activated for {session_key}");
            }
            resume_paid_pipeline(state, session).await;
        }
        _ => resume_paid_pipeline(state, session).await,
    }
}

/// Resume a pipeline parked at AwaitingPayment, given a verified Checkout Session
/// object that carries `metadata.pipeline_id`. Idempotent: a duplicate webhook for
/// an already-resumed pipeline is a no-op (resume_after_payment returns false).
async fn resume_paid_pipeline(state: &Arc<AppState>, session: &serde_json::Value) {
    let pipeline_id_str = session["metadata"]["pipeline_id"].as_str().unwrap_or("");

    match pipeline_id_str.parse::<Uuid>() {
        Ok(pipeline_id) => {
            if state.pipelines.resume_after_payment(pipeline_id).await {
                tracing::info!("[stripe] payment confirmed — resuming pipeline {pipeline_id}");
                pipeline::spawn(pipeline_id, state.pipelines.clone(), state.clone());
            } else {
                // Either already resumed (duplicate event) or never in AwaitingPayment.
                tracing::warn!("[stripe] pipeline {pipeline_id} not in AwaitingPayment state (already resumed?)");
            }
        }
        Err(_) => {
            tracing::warn!("[stripe] invalid pipeline_id in metadata: '{pipeline_id_str}'");
        }
    }
}
