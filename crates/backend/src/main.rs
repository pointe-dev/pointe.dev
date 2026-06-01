use axum::{
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

mod agents;
mod config;
mod email;
mod embeddings;
mod handlers;
mod langfuse;
mod pipeline;
mod pitch;
mod qdrant;
mod sessions;
mod state;
mod stripe;

use embeddings::EmbeddingEngine;
use langfuse::LangfuseClient;
use pipeline::PipelineStore;
use pitch::{PitchResult, PitchStore};
use qdrant::QdrantStore;
use sessions::{SessionStore, FREE_MESSAGES};
use state::AppState;
use stripe::StripeClient;
use sqlx::postgres::PgPoolOptions;

const FALLBACK_PROMPT: &str = "\
Tu es le consultant IA de pointe.dev, une agence d'automatisation sur mesure. \
Tu n'es pas un chatbot générique : tu es un commercial chevronné doublé d'un expert \
technique, le genre de personne qui a déjà libéré des dizaines d'entreprises de leurs \
tâches répétitives et qui met immédiatement son interlocuteur à l'aise. Ton rôle est \
d'aider le prospect à repérer ce qu'il peut déléguer à un collaborateur IA pour gagner \
du temps et de l'argent — puis de lui donner envie d'aller plus loin avec nous.\n\
\n\
Ta posture :\n\
- Parle de délégation, pas de robots : le prospect confie ses corvées à un \
« collaborateur IA » qui le libère. Emploie naturellement « déléguer », « collaborateur \
IA », « vous libérer », « gagner du temps et de l'argent ». Garde « automatisation » \
pour les moments techniques, avec parcimonie.\n\
- Accessible et chaleureux, jamais jargonneux. Tu parles le langage du client, pas \
celui de l'ingénieur. Si tu emploies un terme technique, tu l'expliques en une demi-phrase.\n\
- Curieux et à l'écoute : tu poses UNE question à la fois, ciblée, qui montre que tu \
as compris ce qu'il vient de dire. Une conversation, pas un interrogatoire.\n\
- Concret et crédible : tu illustres avec des exemples de son secteur, tu ne promets \
jamais de chiffres précis que tu ne peux pas tenir. La confiance prime sur l'esbroufe.\n\
- Confiant sans être insistant : tu guides naturellement vers l'étape suivante quand \
le besoin est clair, sans jamais forcer la main.\n\
- Tu restes concis : le prospect doit avoir envie de lire chaque réponse.

Règles absolues :
- Réponds TOUJOURS dans la langue de l'utilisateur (FR, EN ou DE)
- Pose des questions ciblées pour qualifier le besoin : secteur, volume de tâches, taille d'équipe, douleur principale
- Quand l'utilisateur décrit un processus ou workflow, génère OBLIGATOIREMENT un bloc workflow \
  dans le format exact suivant (sans espace avant les backticks) :
```workflow
{\"nodes\":[{\"id\":\"1\",\"label\":\"Étape 1\",\"kind\":\"start\"},{\"id\":\"2\",\"label\":\"Étape 2\",\"kind\":\"process\"}],\"edges\":[{\"from\":\"1\",\"to\":\"2\"}]}
```
- Les nœuds doivent être courts (3-4 mots max), 4-6 nœuds maximum
- Après le workflow, explique brièvement comment pointe.dev automatise ce flux
- Ne jamais halluciner des chiffres précis — utilise des fourchettes réalistes
- Réponse max : 200 mots hors workflow

Options sélectionnables (facultatif) :
Quand tu poses une question dont les réponses courantes sont prévisibles (secteur, volume, outils…), tu PEUX ajouter un bloc options INVISIBLE pour guider l'utilisateur avec des boutons cliquables :
```options
[{\"label\": \"Réponse A\"}, {\"label\": \"Réponse B\"}, {\"label\": \"Réponse C\"}, {\"label\": \"Autre (précisez)\"}]
```
Règles options : 2-4 options MAX, labels courts (3-6 mots), JAMAIS en même temps qu'un bloc qualify ou pitch, JAMAIS obligatoire — uniquement quand ça simplifie vraiment la réponse du prospect.

Déclenchement du pipeline :
Dès que tu as collecté les 4 éléments suivants, INCLUS un bloc qualify INVISIBLE à la fin de ta réponse :
  1. secteur d'activité
  2. douleur principale (tâche répétitive ou source d'erreurs)
  3. outils actuels utilisés (CRM, ERP, e-mail, etc.)
  4. volume approximatif (ex: 50 commandes/jour, 200 leads/mois)

Format du bloc qualify (toujours en dernier, jamais affiché à l'utilisateur) :
```qualify
{\"client_need\": \"une phrase décrivant précisément le besoin d'automatisation\", \"summary\": \"secteur | douleur | outils | volume\"}
```

Immédiatement après le bloc qualify, génère OBLIGATOIREMENT un bloc pitch (jamais sans qualify) :
```pitch
{\"slides\":[{\"title\":\"Ce que nous avons compris\",\"body\":\"...\",\"points\":[\"point clé 1\",\"point clé 2\",\"point clé 3\"]},{\"title\":\"Notre proposition\",\"body\":\"...\",\"points\":[\"Livrable 1 : ...\",\"Livrable 2 : ...\",\"Livrable 3 : ...\"]},{\"title\":\"Prochaines étapes\",\"body\":\"Délai estimé : X jours\",\"points\":[\"Phase 1 : ...\",\"Phase 2 : ...\",\"Mise en production : ...\"]}]}
```
Règles pitch : titres IDENTIQUES aux exemples, body = 1-2 phrases, points = max 10 mots chacun, TOUJOURS dans la langue de l'utilisateur.\n\
\n\
═══════════════════════════════════════════════════\n\
EXEMPLES — étudie le ton et le rythme, ne les recopie jamais mot pour mot\n\
═══════════════════════════════════════════════════\n\
\n\
Exemple de bonne ouverture (chaleureuse, une seule question, ancrée dans le réel) :\n\
Prospect : « Bonjour »\n\
Toi : « Bonjour ! Ravi de vous accueillir. Je suis là pour repérer avec vous où \
l'automatisation pourrait vous faire gagner du temps. Pour commencer simplement : \
c'est quoi, la tâche qui vous prend le plus de temps chaque semaine et que vous \
aimeriez ne plus jamais faire à la main ? »\n\
\n\
Exemple de relance qui montre l'écoute (reformule, puis UNE question ciblée) :\n\
Prospect : « Je passe un temps fou à répondre aux mêmes questions de mes clients par mail. »\n\
Toi : « Je vois exactement le genre — ces e-mails répétitifs qui grignotent la \
journée sans jamais s'arrêter. Pour cerner l'ampleur : vous recevez combien de ces \
demandes par jour, à peu près, et elles arrivent via quel canal — e-mail, formulaire, \
chat sur le site ? »\n\
\n\
Mauvais réflexes à éviter absolument :\n\
- Empiler plusieurs questions d'un coup (« Quel est votre secteur, votre volume, vos \
outils et votre budget ? ») → étouffant, ça casse la conversation.\n\
- Jargon non expliqué (« On va mettre un webhook sur votre CRM via une API REST ») → \
parle d'abord du résultat (« vos nouveaux contacts arrivent tout seuls dans votre \
outil de suivi »).\n\
- Promettre des chiffres précis inventés (« vous gagnerez 73% de temps ») → préfère \
une fourchette honnête (« souvent plusieurs heures par semaine sur ce type de tâche »).\n\
- Proposer un rendez-vous → notre parcours passe par la proposition générée, pas par \
un agenda.\n\
\n\
Exemple complet de fin de qualification (les 4 éléments sont réunis → tu réponds \
normalement au client EN PREMIER, puis tu ajoutes les blocs invisibles) :\n\
\n\
Contexte réuni : boutique en ligne de cosmétiques, recopie manuelle des commandes \
Shopify vers la compta, ~80 commandes/jour.\n\
\n\
Ta réponse visible se termine par une phrase qui invite à découvrir la proposition, \
puis VIENNENT les blocs (jamais montrés au client) :\n\
```qualify\n\
{\"client_need\": \"Synchroniser automatiquement les commandes Shopify vers le logiciel \
de comptabilité pour supprimer la double saisie\", \"summary\": \"e-commerce cosmétiques \
| recopie manuelle des commandes | Shopify, logiciel de compta | ~80 commandes/jour\"}\n\
```\n\
```pitch\n\
{\"slides\":[\
{\"title\":\"Ce que nous avons compris\",\"body\":\"Chaque commande Shopify est \
aujourd'hui recopiée à la main dans votre comptabilité.\",\"points\":[\"~80 commandes \
saisies manuellement/jour\",\"Temps perdu et risque d'erreurs\",\"Aucune visibilité \
temps réel\"]},\
{\"title\":\"Notre proposition\",\"body\":\"Une synchronisation automatique, de la \
commande à l'écriture comptable.\",\"points\":[\"Livrable 1 : connexion Shopify → compta\",\
\"Livrable 2 : création auto des factures\",\"Livrable 3 : alertes en cas d'anomalie\"]},\
{\"title\":\"Prochaines étapes\",\"body\":\"Délai estimé : 5 jours ouvrés.\",\"points\":[\
\"Phase 1 : cadrage et accès\",\"Phase 2 : build et tests\",\"Mise en production\"]}]}\n\
```\n\
\n\
Autre ouverture selon le secteur (adapte, ne récite pas) :\n\
- Cabinet / services pro : « Beaucoup de cabinets perdent un temps fou sur la \
relance des factures et la prise de rendez-vous. Qu'est-ce qui, chez vous, se répète \
le plus souvent à la main ? »\n\
- Industrie / logistique : « Souvent, ce sont les comptes-rendus, les bons de \
livraison ou le suivi des stocks qui mangent les heures. Lequel vous parle le plus ? »\n\
- SaaS / tech : « Entre l'onboarding client, le support de premier niveau et le \
reporting, où sentez-vous le plus de friction aujourd'hui ? »\n\
\n\
Rappel : le client ne voit JAMAIS le contenu des blocs qualify et pitch — ils sont \
captés par l'interface. Ta partie visible reste une réponse humaine, fluide et brève.";

#[derive(Deserialize)]
struct HistoryMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    description: String,
    #[serde(default)]
    history: Vec<HistoryMsg>,
    session_id: String,
    /// SHA-256 hex of browser signals (UA+lang+tz+screen). Used as secondary
    /// rate-limit bucket alongside IP to prevent localStorage-clearing abuse.
    #[serde(default)]
    fingerprint: Option<String>,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
    messages_used: u32,
    messages_free: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<ChatOption>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatOption {
    pub label: String,
}

/// Strips a ```options block from the AI response.
/// Returns (display_text, Vec<ChatOption>).
fn parse_options(text: &str) -> (String, Vec<ChatOption>) {
    const OPEN: &str = "```options";
    const CLOSE: &str = "```";
    if let Some(start) = text.find(OPEN) {
        let after_tag = &text[start + OPEN.len()..];
        let after = match after_tag.find('\n') {
            Some(nl) => &after_tag[nl + 1..],
            None => return (text.to_string(), vec![]),
        };
        if let Some(end) = after.find(CLOSE) {
            let json = after[..end].trim();
            let before = text[..start].trim_end();
            let rest = after[end + CLOSE.len()..].trim_start();
            let display = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{before}\n\n{rest}"),
            };
            let opts = serde_json::from_str::<Vec<ChatOption>>(json).unwrap_or_default();
            return (display, opts);
        }
    }
    (text.to_string(), vec![])
}

#[derive(serde::Deserialize)]
struct QualifyBlock {
    client_need: String,
    summary: String,
}

/// Strips a ```qualify block from the AI response.
/// Returns (display_text, Option<QualifyBlock>).
fn parse_qualify(text: &str) -> (String, Option<QualifyBlock>) {
    const OPEN: &str = "```qualify";
    const CLOSE: &str = "```";
    if let Some(start) = text.find(OPEN) {
        let after_tag = &text[start + OPEN.len()..];
        let after = match after_tag.find('\n') {
            Some(nl) => &after_tag[nl + 1..],
            None => return (text.to_string(), None),
        };
        if let Some(end) = after.find(CLOSE) {
            let json = after[..end].trim();
            let before = text[..start].trim();
            let rest = after[end + CLOSE.len()..].trim();
            let display = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{before}\n\n{rest}"),
            };
            let block = serde_json::from_str::<QualifyBlock>(json).ok();
            return (display, block);
        }
    }
    (text.to_string(), None)
}

#[derive(Deserialize)]
struct UnlockRequest {
    session_id: String,
    email: String,
}

#[derive(Serialize)]
struct UnlockResponse {
    ok: bool,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: &'static str,
    max_tokens: u32,
    /// Structured system block: a single text part carrying cache_control so the
    /// (large) system prompt is cached and re-read across turns of a conversation.
    system: serde_json::Value,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

async fn handle_unlock(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UnlockRequest>,
) -> Json<UnlockResponse> {
    let email = payload.email.trim().to_lowercase();
    let valid = email.contains('@') && email.contains('.');
    if !valid {
        return Json(UnlockResponse { ok: false });
    }

    let confirm_token = SessionStore::sign_confirm_token(&email, &payload.session_id, &state.session_secret);
    let encoded_email = urlencoding::encode(&email);
    let confirm_url = format!(
        "{}/api/auth/confirm?e={}&s={}&t={}",
        state.base_url, encoded_email, payload.session_id, confirm_token
    );

    match &state.resend_api_key {
        Some(api_key) => {
            if let Err(e) = send_confirm_email(&state.http, api_key, &email, &confirm_url).await {
                tracing::error!("Failed to send confirmation email to {email}: {e}");
                return Json(UnlockResponse { ok: false });
            }
        }
        None => {
            // Dev mode: log the link so you can test without a mail provider
            tracing::warn!("RESEND_API_KEY not set — confirm link for {email}: {confirm_url}");
        }
    }

    Json(UnlockResponse { ok: true })
}

#[derive(Deserialize)]
struct ConfirmParams {
    e: String,
    s: String,
    t: String,
}

async fn handle_confirm(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ConfirmParams>,
) -> Redirect {
    let email = params.e.trim().to_lowercase();
    let session_id = params.s.trim().to_string();
    let token = params.t.trim().to_string();

    if email.is_empty() || session_id.is_empty() || token.is_empty() {
        return Redirect::to("/");
    }
    if !email.contains('@') {
        return Redirect::to("/");
    }

    if !SessionStore::verify_confirm_token(&email, &session_id, &token, &state.session_secret) {
        tracing::warn!("Invalid confirmation token for email: {email}");
        return Redirect::to("/");
    }

    let signed = SessionStore::sign_token(&email, &state.session_secret);
    state.sessions.unlock_with_email(&session_id, email.clone(), &signed).await;
    tracing::info!("Email confirmed — session unlocked for: {email}");

    let redirect_url = format!("/?_sid={}", signed);
    Redirect::to(&redirect_url)
}

/// Re-export so handlers in this file can call `resend_send(...)` directly.
use email::resend_send;

async fn send_confirm_email(
    http: &reqwest::Client,
    api_key: &str,
    to_email: &str,
    confirm_url: &str,
) -> Result<(), String> {
    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:sans-serif;background:#0a0a0a;margin:0;padding:40px 20px">
<div style="max-width:480px;margin:auto;background:#111;border-radius:16px;padding:40px;border:1px solid #222">
  <p style="color:#dc2626;font-size:20px;font-weight:700;margin:0 0 20px">pointe.dev</p>
  <h1 style="color:#f3f4f6;font-size:20px;font-weight:600;margin:0 0 12px">Confirmez votre accès</h1>
  <p style="color:#9ca3af;font-size:14px;line-height:1.7;margin:0 0 28px">
    Cliquez sur le bouton ci-dessous pour continuer votre conversation et accéder à notre analyse d'automatisation personnalisée.
  </p>
  <a href="{confirm_url}" style="display:inline-block;background:#dc2626;color:white;padding:14px 28px;border-radius:10px;text-decoration:none;font-weight:600;font-size:15px">
    Continuer la conversation →
  </a>
  <p style="color:#4b5563;font-size:12px;margin-top:32px;line-height:1.6">
    Si vous n'avez pas demandé cet accès, ignorez simplement cet email.
  </p>
</div>
</body></html>"#
    );
    resend_send(http, api_key, to_email, "Continuez votre conversation avec pointe.dev", &html).await
        .inspect(|_| tracing::info!("Confirmation email sent to {to_email}"))
}

// ── Pitch: send proposal by email ────────────────────────────────────────────

#[derive(Deserialize)]
struct SendQuoteRequest {
    email: String,
    slides: Vec<serde_json::Value>,
}

async fn handle_send_quote(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SendQuoteRequest>,
) -> Json<serde_json::Value> {
    let api_key = match &state.resend_api_key {
        Some(k) => k.clone(),
        None => {
            tracing::warn!("[quote] RESEND_API_KEY not set");
            return Json(serde_json::json!({ "ok": false, "error": "email not configured" }));
        }
    };

    let client_email = payload.email.trim().to_lowercase();
    if !client_email.contains('@') {
        return Json(serde_json::json!({ "ok": false, "error": "invalid email" }));
    }

    // Build slide HTML
    let slides_html = payload.slides.iter().map(|s| {
        let title  = s["title"].as_str().unwrap_or("");
        let body   = s["body"].as_str().unwrap_or("");
        let points = s["points"].as_array().map(|arr|
            arr.iter().map(|p| format!(
                "<li style='margin:4px 0;color:#d1d5db'>{}</li>",
                p.as_str().unwrap_or("")
            )).collect::<String>()
        ).unwrap_or_default();
        format!(
            "<div style='margin-bottom:24px'>\
               <h3 style='color:#f3f4f6;font-size:15px;margin:0 0 6px'>{title}</h3>\
               <p style='color:#9ca3af;font-size:13px;margin:0 0 8px'>{body}</p>\
               {}</div>",
            if points.is_empty() { String::new() }
            else { format!("<ul style='margin:0;padding-left:20px'>{points}</ul>") }
        )
    }).collect::<String>();

    let client_html = format!(
        r#"<!DOCTYPE html><html><body style="font-family:sans-serif;background:#0a0a0a;margin:0;padding:40px 20px">
<div style="max-width:560px;margin:auto;background:#111;border-radius:16px;padding:40px;border:1px solid #222">
  <p style="color:#dc2626;font-size:20px;font-weight:700;margin:0 0 24px">pointe.dev</p>
  <h1 style="color:#f3f4f6;font-size:20px;font-weight:600;margin:0 0 24px">Votre proposition</h1>
  {slides_html}
  <p style="color:#6b7280;font-size:12px;margin-top:28px">Notre équipe vous contactera prochainement avec une estimation détaillée.</p>
</div></body></html>"#
    );

    // Notify owner in background
    if let Some(owner) = &state.owner_email {
        let owner_html = format!(
            "<div style='font-family:sans-serif;padding:24px'>\
               <h2>Nouvelle proposition demandée</h2>\
               <p><b>Client :</b> {client_email}</p>\
             </div>"
        );
        let http2  = state.http.clone();
        let api2   = api_key.clone();
        let owner  = owner.clone();
        tokio::spawn(async move {
            if let Err(e) = resend_send(&http2, &api2, &owner,
                "🎯 Nouvelle proposition demandée — pointe.dev", &owner_html).await {
                tracing::warn!("[quote] owner notify failed: {e}");
            }
        });
    }

    match resend_send(&state.http, &api_key, &client_email,
        "Votre proposition pointe.dev", &client_html).await {
        Ok(()) => {
            tracing::info!("[quote] proposal sent to {client_email}");
            Json(serde_json::json!({ "ok": true }))
        }
        Err(e) => {
            tracing::error!("[quote] send failed: {e}");
            Json(serde_json::json!({ "ok": false, "error": e }))
        }
    }
}

// ── Pitch: n8n pipeline callback ─────────────────────────────────────────────

#[derive(Deserialize)]
struct PipelineResultPayload {
    session_id: String,
    #[serde(flatten)]
    result: PitchResult,
}

async fn handle_pipeline_result(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PipelineResultPayload>,
) -> Json<serde_json::Value> {
    if payload.session_id.is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "missing session_id" }));
    }
    state.pitches.set(&payload.session_id, payload.result).await;
    Json(serde_json::json!({ "ok": true }))
}

// ── Pitch: frontend polling ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct PitchPollParams {
    sid: String,
}

async fn handle_pitch_poll(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PitchPollParams>,
) -> Json<serde_json::Value> {
    match state.pitches.get(&params.sid).await {
        Some(r) => Json(serde_json::json!({
            "ready":            true,
            "manual_quote":     r.manual_quote,
            "solution_desc":    r.solution_desc,
            "price_eur_cents":  r.price_eur_cents,
            "price_validity":   r.price_validity,
            "externals_needed": r.externals_needed,
            "slides":           r.slides,
        })),
        None => Json(serde_json::json!({ "ready": false })),
    }
}

// ── Auth: email confirmation status (frontend poll) ───────────────────────────

#[derive(Deserialize)]
struct AuthStatusParams { sid: String }

async fn handle_auth_status(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuthStatusParams>,
) -> Json<serde_json::Value> {
    let unlocked = state.sessions.is_unlocked(&params.sid).await;
    let email    = state.sessions.get_email(&params.sid).await;
    Json(serde_json::json!({ "unlocked": unlocked, "email": email }))
}

/// Extract the best-guess real IP from the request.
fn real_ip(addr: SocketAddr, headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string())
}

async fn handle_ai_chat(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let session_key = payload.session_id.clone();
    let ip = real_ip(addr, &headers);
    let fp_key = payload.fingerprint.as_deref().map(|fp| SessionStore::fp_bucket(&ip, fp));
    if !state.sessions.check_and_increment(&session_key, fp_key.as_deref()).await {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    let messages_used = state.sessions.message_count(&session_key).await;

    let start = Utc::now();

    let messages: Vec<AnthropicMessage> = payload.history.into_iter()
        .map(|h| AnthropicMessage { role: h.role, content: h.content })
        .chain(std::iter::once(AnthropicMessage {
            role: "user".to_string(),
            content: payload.description.clone(),
        }))
        .collect();

    let body = AnthropicRequest {
        // Sonnet for the conversational qualifier: markedly more natural/persuasive
        // than Haiku, and its ~1024-token cache minimum (vs Haiku's ~4096, measured)
        // means our ~2240-token system prompt caches at its current size.
        model: "claude-sonnet-4-6",
        max_tokens: 1024,
        // Cache breakpoint on the system prompt (identical across conversation turns,
        // which happen seconds apart → high hit rate within a conversation).
        system: serde_json::json!([{
            "type": "text",
            "text": state.system_prompt,
            "cache_control": { "type": "ephemeral", "ttl": "1h" }
        }]),
        messages,
    };

    let resp = state
        .http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &state.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "prompt-caching-2024-07-31,extended-cache-ttl-2025-04-11")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Anthropic request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!("Anthropic error {status}: {text}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let raw = resp.text().await.map_err(|e| {
        tracing::error!("Anthropic read error: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    let ant_resp: AnthropicResponse = serde_json::from_str(&raw).map_err(|e| {
        tracing::error!("Anthropic parse error: {e} — body: {raw}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(u) = &ant_resp.usage {
        tracing::info!(
            "[chat] tokens in={} cache_write={} cache_read={} (hit_ratio={:.0}%)",
            u.input_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens,
            {
                let total = u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens;
                if total > 0 { u.cache_read_input_tokens as f64 / total as f64 * 100.0 } else { 0.0 }
            }
        );
    }

    let raw_text = ant_resp.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();
    let end = Utc::now();

    // Strip qualify block and launch pipeline if the AI decided to qualify
    let (display_text, pipeline_id, options) = {
        let (after_qualify, maybe_qualify) = parse_qualify(&raw_text);
        let pid = if let Some(q) = maybe_qualify {
            let id = state.pipelines.create(
                payload.session_id.clone(),
                q.client_need,
                Some(q.summary),
            ).await;
            pipeline::spawn(id, state.pipelines.clone(), state.clone());
            tracing::info!("Pipeline {id} launched from chat session={}", payload.session_id);
            Some(id.to_string())
        } else {
            None
        };
        let (display, opts) = parse_options(&after_qualify);
        (display, pid, opts)
    };

    if state.langfuse.is_some() {
        let input = payload.description.clone();
        let output = display_text.clone();
        let state2 = state.clone();
        tokio::spawn(async move {
            if let Some(lf) = &state2.langfuse {
                lf.trace(&input, &output, "claude-sonnet-4-6", start, end).await;
            }
        });
    }

    Ok(Json(ChatResponse {
        response: display_text,
        messages_used,
        messages_free: FREE_MESSAGES,
        pipeline_id,
        options,
    }))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();

    let http = reqwest::Client::new();
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");

    let (system_prompt, langfuse) = init_langfuse(http.clone()).await;

    let qdrant = std::env::var("QDRANT_URL").ok().map(|url| {
        tracing::info!("Qdrant configured at {url}");
        QdrantStore::new(http.clone(), url)
    });
    if qdrant.is_none() {
        tracing::warn!("QDRANT_URL not set — RAG disabled, builder uses stub");
    }

    // BGE-M3 only needed when Qdrant RAG is active.
    let embeddings = if qdrant.is_some() {
        tokio::task::spawn_blocking(EmbeddingEngine::new)
            .await
            .unwrap_or_else(|e| Err(format!("join error: {e}")))
            .map_err(|e| tracing::warn!("Embedding engine unavailable: {e} — RAG disabled"))
            .ok()
    } else {
        None
    };
    if embeddings.is_some() {
        tracing::info!("Embedding engine ready (BGE-M3, 1024 dims, local)");
    }

    let stripe = match (
        std::env::var("STRIPE_SECRET_KEY").ok(),
        std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
    ) {
        (Some(sk), Some(wh)) => {
            tracing::info!("Stripe configured");
            Some(StripeClient::new(http.clone(), sk, wh))
        }
        _ => {
            tracing::warn!("STRIPE_SECRET_KEY or STRIPE_WEBHOOK_SECRET not set — payments disabled");
            None
        }
    };

    let session_secret = config::load_session_secret();
    let admin_ingest_token = config::load_admin_ingest_token();
    if admin_ingest_token.is_none() {
        tracing::warn!("ADMIN_INGEST_TOKEN not set — /api/admin/ingest will reject requests");
    }

    let resend_api_key = std::env::var("RESEND_API_KEY").ok();
    if resend_api_key.is_none() {
        tracing::warn!("RESEND_API_KEY not set — confirmation links will be logged instead of emailed");
    }

    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3001".to_string());
    tracing::info!("Base URL: {base_url}");

    let owner_email = std::env::var("OWNER_EMAIL").ok();
    if let Some(ref e) = owner_email {
        tracing::info!("Owner notifications → {e}");
    }

    let db = match std::env::var("DATABASE_URL") {
        Ok(url) => {
            match PgPoolOptions::new().max_connections(5).connect(&url).await {
                Ok(pool) => {
                    if let Err(e) = pitch::run_migrations(&pool).await {
                        tracing::warn!("DB migration failed: {e} — falling back to in-memory");
                        None
                    } else {
                        tracing::info!("Postgres connected — pitch persistence enabled");
                        Some(pool)
                    }
                }
                Err(e) => {
                    tracing::warn!("DATABASE_URL set but connection failed: {e} — in-memory only");
                    None
                }
            }
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL not set — pitches stored in-memory only (lost on restart)");
            None
        }
    };

    let state = Arc::new(AppState {
        anthropic_key,
        http,
        system_prompt,
        langfuse,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        pitches: PitchStore::new(db.clone()),
        qdrant,
        embeddings,
        stripe,
        session_secret,
        admin_ingest_token,
        resend_api_key,
        base_url,
        owner_email,
        db,
    });

    let app = Router::new()
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .route("/api/auth/unlock", post(handle_unlock))
        .route("/api/auth/confirm", get(handle_confirm))
        .route("/api/pitch/send-quote", post(handle_send_quote))
        .route("/api/pitch/pipeline-result", post(handle_pipeline_result))
        .route("/api/pitch/result", get(handle_pitch_poll))
        .route("/api/auth/status", get(handle_auth_status))
        .route("/api/pipeline/start", post(handlers::pipeline::start))
        .route("/api/pipeline/:id", get(handlers::pipeline::status))
        .route("/api/pipeline/:id/resume", post(handlers::pipeline::resume))
        .route("/api/admin/ingest", post(handlers::ingest::ingest))
        .route("/api/stripe/checkout", post(handlers::stripe::create_checkout))
        .route("/api/stripe/webhook", post(handlers::stripe::webhook))
        .route("/mcp", post(handlers::mcp::handle))
        .route("/merci", get(serve_index))
        .with_state(state)
        .nest(
            "/pkg",
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend/pkg"))
                .layer(middleware::from_fn(no_store)),
        )
        .fallback_service(
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend"))
                .layer(middleware::from_fn(no_store)),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new());

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind");

    tracing::info!("✨ pointe.dev listening on http://{bind_addr}");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Server error");
}

async fn serve_index() -> impl axum::response::IntoResponse {
    match tokio::fs::read("./crates/frontend/index.html").await {
        Ok(bytes) => axum::response::Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(axum::body::Body::from(bytes))
            .unwrap(),
        Err(_) => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap(),
    }
}

async fn no_store(req: axum::extract::Request, next: Next) -> axum::response::Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

async fn init_langfuse(http: reqwest::Client) -> (String, Option<LangfuseClient>) {
    let (Some(pub_key), Some(sec_key), Some(base_url)) = (
        std::env::var("LANGFUSE_PUBLIC_KEY").ok(),
        std::env::var("LANGFUSE_SECRET_KEY").ok(),
        std::env::var("LANGFUSE_BASE_URL").ok(),
    ) else {
        tracing::warn!("Langfuse keys not set, using fallback prompt");
        return (FALLBACK_PROMPT.to_string(), None);
    };

    let mut client = LangfuseClient::new(http, base_url, pub_key, sec_key);
    match client.fetch_prompt("qualifier-chatbot-prompt").await {
        Ok(prompt) => {
            tracing::info!(
                "Loaded Langfuse prompt '{}' v{}",
                client.prompt_name,
                client.prompt_version
            );
            (prompt, Some(client))
        }
        Err(e) => {
            tracing::warn!("Failed to fetch Langfuse prompt: {e} — using fallback");
            (FALLBACK_PROMPT.to_string(), Some(client))
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no HTTP
// Covers  : parse_qualify() — block extraction, display-text trimming,
//           before+after text reconstruction, JSON parse failure, absent block
// Does NOT cover: the AI response generation, Anthropic API, session handling,
//                 email confirmation flow
#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_qualify ──────────────────────────────────────────────────────

    #[test]
    fn parse_qualify_extracts_block_and_strips_display() {
        let text = r#"Here is my answer.
```qualify
{"client_need":"automate orders","summary":"sector|pain|tools|volume"}
```"#;
        let (display, block) = parse_qualify(text);
        assert_eq!(display.trim(), "Here is my answer.");
        let b = block.expect("block must be present");
        assert_eq!(b.client_need, "automate orders");
        assert_eq!(b.summary, "sector|pain|tools|volume");
    }

    #[test]
    fn parse_qualify_no_block_returns_text_unchanged() {
        let text = "Just a normal AI response.";
        let (display, block) = parse_qualify(text);
        assert_eq!(display, text);
        assert!(block.is_none());
    }

    #[test]
    fn parse_qualify_invalid_json_returns_none_block() {
        let text = "Before\n```qualify\nnot-valid-json\n```\nAfter";
        let (_, block) = parse_qualify(text);
        assert!(block.is_none(), "malformed JSON must not crash");
    }

    #[test]
    fn parse_qualify_preserves_before_and_after_text() {
        let text = "BEFORE\n```qualify\n{\"client_need\":\"n\",\"summary\":\"s\"}\n```\nAFTER";
        let (display, block) = parse_qualify(text);
        assert!(display.contains("BEFORE"));
        assert!(display.contains("AFTER"));
        assert!(block.is_some());
    }

    #[test]
    fn parse_qualify_empty_display_when_only_block() {
        let text = "```qualify\n{\"client_need\":\"n\",\"summary\":\"s\"}\n```";
        let (display, _) = parse_qualify(text);
        // Display should be empty (no content outside the block)
        assert!(display.is_empty());
    }

    #[test]
    fn parse_qualify_block_only_before() {
        let text = "Visible text\n```qualify\n{\"client_need\":\"x\",\"summary\":\"y\"}\n```";
        let (display, block) = parse_qualify(text);
        assert_eq!(display.trim(), "Visible text");
        assert!(block.is_some());
    }

    // ── parse_options ──────────────────────────────────────────────────────

    #[test]
    fn parse_options_extracts_labels_and_strips_block() {
        let text = "Quel est votre secteur ?\n```options\n[{\"label\":\"E-commerce\"},{\"label\":\"Santé\"},{\"label\":\"Autre\"}]\n```";
        let (display, opts) = parse_options(text);
        assert_eq!(display.trim(), "Quel est votre secteur ?");
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].label, "E-commerce");
        assert_eq!(opts[2].label, "Autre");
    }

    #[test]
    fn parse_options_no_block_returns_text_unchanged() {
        let text = "A normal reply with no options.";
        let (display, opts) = parse_options(text);
        assert_eq!(display, text);
        assert!(opts.is_empty());
    }

    #[test]
    fn parse_options_invalid_json_returns_empty_vec() {
        let text = "Before\n```options\nnot-json\n```\nAfter";
        let (_, opts) = parse_options(text);
        assert!(opts.is_empty(), "malformed JSON must degrade gracefully");
    }

    #[test]
    fn parse_options_preserves_before_and_after_text() {
        let text = "BEFORE\n```options\n[{\"label\":\"A\"}]\n```\nAFTER";
        let (display, opts) = parse_options(text);
        assert!(display.contains("BEFORE"));
        assert!(display.contains("AFTER"));
        assert_eq!(opts.len(), 1);
    }

    #[test]
    fn parse_options_empty_array_yields_no_options() {
        let text = "Pick one\n```options\n[]\n```";
        let (display, opts) = parse_options(text);
        assert_eq!(display.trim(), "Pick one");
        assert!(opts.is_empty());
    }

    // ── real_ip ────────────────────────────────────────────────────────────

    #[test]
    fn real_ip_prefers_x_forwarded_for() {
        use axum::http::HeaderMap;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.1, 10.0.0.1".parse().unwrap());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4321);
        let ip = real_ip(addr, &headers);
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn real_ip_falls_back_to_socket_addr() {
        use axum::http::HeaderMap;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let headers = HeaderMap::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5678);
        let ip = real_ip(addr, &headers);
        assert_eq!(ip, "192.168.1.1");
    }
}
