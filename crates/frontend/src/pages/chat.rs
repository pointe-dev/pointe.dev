use leptos::*;
use serde::{Deserialize, Serialize};
use leptos::spawn_local;
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;
use crate::components::consent_banner::is_consent_given;
use crate::i18n::{Lang, t};

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve: js_sys::Function, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}

const FREE_MESSAGES: u32 = 5;

/// Returns (session_id, is_token_session).
/// Checks URL ?_sid param first (set by email confirmation redirect), then localStorage.
fn get_or_create_session_id() -> (String, bool) {
    // Email confirmation redirect: ?_sid=<64-hex-token>
    let url_sid = js_sys::eval(
        "(function(){\
           try{\
             var p=new URLSearchParams(window.location.search);\
             var sid=p.get('_sid');\
             if(sid&&sid.length===64&&/^[0-9a-f]+$/.test(sid)){\
               localStorage.setItem('_sid',sid);\
               history.replaceState(null,'',window.location.pathname);\
               return sid;\
             }\
           }catch(e){}\
           return '';\
         })()"
    ).ok().and_then(|v| v.as_string()).filter(|s| !s.is_empty());

    if let Some(sid) = url_sid {
        return (sid, true);
    }

    let window = web_sys::window().unwrap();
    let storage = window.local_storage().unwrap().unwrap();
    if let Ok(Some(id)) = storage.get_item("_sid") {
        let is_token = id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit());
        return (id, is_token);
    }
    let id = js_sys::eval(
        "'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g,\
         function(c){var r=Math.random()*16|0,v=c=='x'?r:(r&0x3|0x8);return v.toString(16)})"
    ).ok()
     .and_then(|v| v.as_string())
     .unwrap_or_else(|| format!("{}", js_sys::Date::now() as u64));
    let _ = storage.set_item("_sid", &id);
    (id, false)
}

/// Collect browser signals and produce a simple djb2 hex hash.
/// Sent as `fingerprint` with each chat request for IP+fingerprint rate limiting.
fn compute_fingerprint() -> String {
    let fp_str = js_sys::eval(
        "(function(){\
           var ua=navigator.userAgent||'';\
           var lang=navigator.language||'';\
           var tz='';\
           try{tz=Intl.DateTimeFormat().resolvedOptions().timeZone||'';}catch(e){}\
           var sw=screen.width||0,sh=screen.height||0;\
           return ua+'|'+lang+'|'+tz+'|'+sw+'x'+sh;\
         })()"
    ).ok().and_then(|v| v.as_string()).unwrap_or_default();
    // djb2 hash — good enough for a non-crypto bucket key
    let mut h: u64 = 5381;
    for b in fp_str.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    format!("{:016x}", h)
}

#[derive(Serialize)]
struct HistoryMsg {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    description: String,
    history: Vec<HistoryMsg>,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    response: String,
    messages_used: u32,
    messages_free: u32,
    pipeline_id: Option<String>,
    #[serde(default)]
    options: Vec<ChatOption>,
    /// Set when the visitor qualified but must confirm their email before the
    /// pipeline runs. The frontend opens the email modal in response.
    #[serde(default)]
    needs_unlock: bool,
}

#[derive(Deserialize, Clone, Debug)]
struct ChatOption {
    label: String,
}

#[derive(Serialize)]
struct UnlockRequest {
    session_id: String,
    email: String,
}

#[derive(Deserialize)]
struct UnlockResponse {
    ok: bool,
}

#[derive(Serialize)]
struct CheckoutRequest {
    pipeline_id: String,
}

#[derive(Deserialize)]
struct CheckoutResponse {
    checkout_url: String,
}

#[derive(Deserialize, Clone)]
struct PitchSlide {
    title: String,
    body: String,
    #[serde(default)]
    points: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct PitchData {
    slides: Vec<PitchSlide>,
}

#[derive(Deserialize)]
struct PitchPollResponse {
    ready: bool,
    #[serde(default)]
    manual_quote: bool,
    #[serde(default)]
    slides: Vec<PitchSlide>,
    #[serde(default)]
    price_eur_cents: u32,
    #[serde(default)]
    price_validity: String,
}

#[derive(Deserialize)]
struct AuthStatusResponse {
    unlocked: bool,
    /// Present once a gated pipeline was spawned on confirm — the polling tab
    /// uses it to start watching the pitch without a page reload.
    #[serde(default)]
    pipeline_id: Option<String>,
}

#[derive(Deserialize)]
struct PipelineStatusResponse {
    /// Tagged enum: `{"stage":"failed","reason":...}` etc.
    stage: serde_json::Value,
}

fn strip_block<'a>(content: &'a str, tag: &str) -> (&'a str, Option<&'a str>) {
    let open = Box::leak(format!("```{}", tag).into_boxed_str()) as &str;
    let close = "\n```";
    if let Some(s) = content.find(open) {
        let after_tag = &content[s + open.len()..];
        if let Some(nl) = after_tag.find('\n') {
            let after = &after_tag[nl + 1..];
            if let Some(e) = after.find(close) {
                let block = &after[..e];
                return (&content[..s], Some(block));
            }
        }
    }
    (content, None)
}

fn parse_response(content: &str) -> (String, Option<PitchData>, Vec<ChatOption>) {
    // Strip pitch block
    let (without_pitch, pitch_json) = {
        const OPEN: &str = "```pitch";
        const CLOSE: &str = "\n```";
        if let Some(s) = content.find(OPEN) {
            let after_tag = &content[s + OPEN.len()..];
            let after = after_tag.find('\n').map(|nl| &after_tag[nl + 1..]).unwrap_or("");
            if let Some(e) = after.find(CLOSE) {
                let json = &after[..e];
                let before = content[..s].trim_end();
                let rest = after[e + CLOSE.len()..].trim_start();
                let text = match (before.is_empty(), rest.is_empty()) {
                    (true,  true)  => String::new(),
                    (false, true)  => before.to_string(),
                    (true,  false) => rest.to_string(),
                    (false, false) => format!("{}\n\n{}", before, rest),
                };
                let pitch = serde_json::from_str::<PitchData>(json).ok();
                (text, pitch)
            } else {
                (content.to_string(), None)
            }
        } else {
            (content.to_string(), None)
        }
    };

    // Strip workflow block
    let without_workflow = {
        const OPEN: &str = "```workflow";
        const CLOSE: &str = "\n```";
        if let Some(s) = without_pitch.find(OPEN) {
            let after_tag = &without_pitch[s + OPEN.len()..];
            let after = after_tag.find('\n').map(|nl| &after_tag[nl + 1..]).unwrap_or("");
            if let Some(e) = after.find(CLOSE) {
                let before = without_pitch[..s].trim_end();
                let rest = after[e + CLOSE.len()..].trim_start();
                match (before.is_empty(), rest.is_empty()) {
                    (true,  true)  => String::new(),
                    (false, true)  => before.to_string(),
                    (true,  false) => rest.to_string(),
                    (false, false) => format!("{}\n\n{}", before, rest),
                }
            } else {
                without_pitch
            }
        } else {
            without_pitch
        }
    };

    // Strip options block and extract options
    let (display_text, options) = {
        const OPEN: &str = "```options";
        const CLOSE: &str = "\n```";
        if let Some(s) = without_workflow.find(OPEN) {
            let after_tag = &without_workflow[s + OPEN.len()..];
            let after = after_tag.find('\n').map(|nl| &after_tag[nl + 1..]).unwrap_or("");
            if let Some(e) = after.find(CLOSE) {
                let json = &after[..e];
                let before = without_workflow[..s].trim_end();
                let rest = after[e + CLOSE.len()..].trim_start();
                let text = match (before.is_empty(), rest.is_empty()) {
                    (true,  true)  => String::new(),
                    (false, true)  => before.to_string(),
                    (true,  false) => rest.to_string(),
                    (false, false) => format!("{}\n\n{}", before, rest),
                };
                let opts = serde_json::from_str::<Vec<ChatOption>>(json).unwrap_or_default();
                (text, opts)
            } else {
                (without_workflow, vec![])
            }
        } else {
            (without_workflow, vec![])
        }
    };

    (display_text, pitch_json, options)
}

#[derive(Serialize, Deserialize)]
struct StoredMsg {
    is_user: bool,
    content: String,
}

fn load_history() -> Vec<(bool, String, String)> {
    if !is_consent_given() { return vec![]; }
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("chat_msgs").ok().flatten())
        .and_then(|json| serde_json::from_str::<Vec<StoredMsg>>(&json).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let html = if m.is_user { m.content.clone() } else { render_markdown(&m.content) };
            (m.is_user, m.content, html)
        })
        .collect()
}

fn save_history(messages: &[(bool, String, String)]) {
    if !is_consent_given() { return; }
    let stored: Vec<StoredMsg> = messages.iter()
        .map(|(is_user, raw, _)| StoredMsg { is_user: *is_user, content: raw.clone() })
        .collect();
    if let Ok(json) = serde_json::to_string(&stored) {
        if let Some(w) = web_sys::window() {
            if let Ok(Some(s)) = w.local_storage() {
                let _ = s.set_item("chat_msgs", &json);
            }
        }
    }
}

/// Persists the in-flight pitch pipeline id so polling can resume after a
/// page refresh. The pitch payload itself lives server-side (Postgres), so we
/// only need the id to re-poll `/api/pitch/result`.
fn save_pitch_pipeline(id: &Option<String>) {
    if !is_consent_given() { return; }
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        match id {
            Some(v) => { let _ = s.set_item("pitch_pipeline_id", v); }
            None    => { let _ = s.remove_item("pitch_pipeline_id"); }
        }
    }
}

fn load_pitch_pipeline() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("pitch_pipeline_id").ok().flatten())
        .filter(|v| !v.is_empty())
}

/// Maps a backend pipeline stage (snake_case from PipelineStage) to a
/// client-facing label shown on the loading button while the proposal is built.
fn stage_label(stage: &str) -> &'static str {
    match stage {
        "qualifying"         => "Qualification…",
        "researching"        => "Recherche en cours…",
        "building"           => "Conception du workflow…",
        "validating"         => "Vérification…",
        "pricing"            => "Chiffrage…",
        "pricing_validating" => "Validation du tarif…",
        "deploying"          => "Préparation…",
        _                    => "Analyse en cours…",
    }
}

fn render_markdown(input: &str) -> String {
    use pulldown_cmark::{html::push_html, Options, Parser};
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(input, opts);
    let mut out = String::new();
    push_html(&mut out, parser);
    out
}

fn copy_text(text: &str) {
    let _ = js_sys::Function::new_with_args(
        "t",
        "try{navigator.clipboard.writeText(t)}catch(e){\
         var ta=document.createElement('textarea');\
         ta.value=t;document.body.appendChild(ta);\
         ta.select();document.execCommand('copy');\
         document.body.removeChild(ta);}"
    ).call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::from_str(text));
}

#[component]
pub fn Chat() -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    let (session_id, is_token_session) = get_or_create_session_id();
    let session_id_send = session_id.clone();
    let session_id_unlock = session_id.clone();
    let is_unlocked: RwSignal<bool> = create_rw_signal(is_token_session);

    let messages = create_rw_signal::<Vec<(bool, String, String)>>(load_history());
    let input_text    = create_rw_signal(String::new());
    let is_loading    = create_rw_signal(false);
    let copied_idx: RwSignal<Option<usize>> = create_rw_signal(None);
    let current_pitch: RwSignal<Option<PitchData>> = create_rw_signal(None);
    let messages_used: RwSignal<u32> = create_rw_signal(0);
    let show_unlock: RwSignal<bool> = create_rw_signal(false);
    // Why the unlock modal is open: false = free-message quota hit,
    // true = qualified and the pipeline is gated behind email confirmation.
    let unlock_for_pipeline: RwSignal<bool> = create_rw_signal(false);
    let email_input = create_rw_signal(String::new());
    let unlock_error = create_rw_signal(false);
    let email_pending = create_rw_signal(false);
    let pipeline_id: RwSignal<Option<String>> = create_rw_signal(None);
    let show_pitch: RwSignal<bool> = create_rw_signal(false);
    let pitch_page: RwSignal<usize> = create_rw_signal(0);

    // Selectable options shown below the last assistant message
    let pending_options: RwSignal<Vec<ChatOption>> = create_rw_signal(vec![]);

    // Cold visitor (no saved history) → seed starter prompts so the chat is
    // never a blank page. The welcome line is rendered as an empty-state bubble
    // (in the view), deliberately NOT pushed into `messages` — keeping it out of
    // saved history and out of the conversation sent to the qualifier.
    if messages.get_untracked().is_empty() {
        let l = lang.get_untracked();
        pending_options.set(vec![
            ChatOption { label: t(l, "chat.starter1").to_string() },
            ChatOption { label: t(l, "chat.starter2").to_string() },
            ChatOption { label: t(l, "chat.starter3").to_string() },
        ]);
    }

    // Pitch pipeline polling
    let pitch_loading: RwSignal<bool>         = create_rw_signal(false);
    let pitch_manual_quote: RwSignal<bool>    = create_rw_signal(false);
    let pitch_price_cents: RwSignal<u32>      = create_rw_signal(0);
    let pitch_price_validity: RwSignal<String> = create_rw_signal(String::new());
    let pitch_poll_tick: RwSignal<u32>        = create_rw_signal(0);
    // Stripe checkout (priced pitches only)
    let checkout_pending: RwSignal<bool>      = create_rw_signal(false);
    let checkout_error: RwSignal<bool>        = create_rw_signal(false);
    // Set when the pipeline hard-fails (agent error / record gone after restart)
    // so we stop the spinner and show a real message instead of polling forever.
    let pitch_failed: RwSignal<bool>          = create_rw_signal(false);
    // Live pipeline stage label shown on the loading button (streamed via polling).
    let pitch_stage: RwSignal<String>         = create_rw_signal("Analyse en cours…".to_string());

    // Resume a pitch pipeline that was in flight (or completed) before a refresh.
    // The poll below resolves it to ready / failed via the server.
    if let Some(pid) = load_pitch_pipeline() {
        pipeline_id.set(Some(pid));
        pitch_loading.set(true);
    }

    // Email confirmation polling
    let email_confirmed: RwSignal<bool> = create_rw_signal(false);
    let email_poll_tick: RwSignal<u32>  = create_rw_signal(0);

    // Toast notification
    let toast_msg: RwSignal<Option<String>> = create_rw_signal(None);

    // Focus textarea on mount
    create_effect(move |first| {
        if first.is_none() { return; }
        let _ = js_sys::Function::new_no_args(
            "var t=document.querySelector('.chat-textarea');if(t)t.focus();"
        ).call0(&wasm_bindgen::JsValue::NULL);
    });

    // Persist conversation to localStorage on every change
    create_effect(move |_| {
        save_history(&messages.get());
    });

    // Persist the in-flight pitch pipeline id so polling resumes after refresh
    create_effect(move |_| {
        save_pitch_pipeline(&pipeline_id.get());
    });

    create_effect(move |_| {
        let _ = messages.get();
        let _ = is_loading.get();
        // rAF ensures DOM is painted before we measure scrollHeight
        let _ = js_sys::Function::new_no_args(
            "requestAnimationFrame(function(){\
               var c=document.querySelector('.chat-scroll');\
               if(c)c.scrollTop=c.scrollHeight;\
             });"
        ).call0(&wasm_bindgen::JsValue::NULL);
    });

    // ── Pitch result poll ─────────────────────────────────────────────────────
    // Resolves to one of three terminal states (never spins forever):
    //   • ready  → pitch published (served from Postgres, survives restart)
    //   • failed → pipeline marked `failed`, its record is gone (restart), or
    //              the safety-net timeout fired
    //   • else   → keep polling
    create_effect(move |_| {
        let attempt = pitch_poll_tick.get();
        if !pitch_loading.get_untracked() { return; }
        let pid = pipeline_id.get_untracked();
        spawn_local(async move {
            sleep_ms(3000).await;

            // 1) Pitch ready? Keyed by pipeline id, so each qualification polls
            //    its own result (a re-qualification never sees a previous one).
            if let Some(pid_str) = pid.clone() {
            if let Ok(r) = Request::get(&format!("/api/pitch/result?pid={}", pid_str)).send().await {
                if let Ok(data) = r.json::<PitchPollResponse>().await {
                    if data.ready {
                        let slides       = data.slides;
                        let manual_quote = data.manual_quote;
                        let cents        = data.price_eur_cents;
                        let validity     = data.price_validity;
                        batch(move || {
                            current_pitch.set(Some(PitchData { slides }));
                            pitch_manual_quote.set(manual_quote);
                            pitch_price_cents.set(cents);
                            pitch_price_validity.set(validity);
                            pitch_loading.set(false);
                        });
                        return;
                    }
                }
            }
            }

            // 2) Read pipeline status: surface the live stage on the button, and
            //    detect a hard failure (marked `failed`, or the record is gone
            //    after a restart — can't recover that run).
            if let Some(pid) = pid {
                if let Ok(resp) = Request::get(&format!("/api/pipeline/{}", pid)).send().await {
                    if resp.status() == 404 {
                        if attempt >= 2 { // tolerate a brief startup race, then give up
                            batch(move || { pitch_failed.set(true); pitch_loading.set(false); });
                            return;
                        }
                    } else if let Ok(s) = resp.json::<PipelineStatusResponse>().await {
                        let stage = s.stage.get("stage").and_then(|v| v.as_str()).unwrap_or("");
                        if stage == "failed" {
                            batch(move || { pitch_failed.set(true); pitch_loading.set(false); });
                            return;
                        }
                        pitch_stage.set(stage_label(stage).to_string());
                    }
                }
            }

            // 3) Safety net (~5 min) — never poll indefinitely.
            if attempt >= 100 {
                batch(move || { pitch_failed.set(true); pitch_loading.set(false); });
                return;
            }

            pitch_poll_tick.update(|t| *t += 1);
        });
    });

    // ── Email confirmation poll ───────────────────────────────────────────────
    let sid_email = session_id.clone();
    // StoredValue is Copy — can be captured by multiple reactive closures
    let session_id_pitch_form = store_value(session_id.clone());
    create_effect(move |_| {
        let _tick = email_poll_tick.get();
        if !email_pending.get_untracked() || email_confirmed.get_untracked() { return; }
        let sid = sid_email.clone();
        spawn_local(async move {
            sleep_ms(3000).await;
            match Request::get(&format!("/api/auth/status?sid={}", sid)).send().await {
                Ok(r) => match r.json::<AuthStatusResponse>().await {
                    Ok(s) if s.unlocked => {
                        let gated_pipeline = s.pipeline_id;
                        batch(move || {
                            email_confirmed.set(true);
                            is_unlocked.set(true);
                            // A gated pipeline was spawned on confirm — start
                            // watching the pitch without a page reload.
                            if let Some(pid) = gated_pipeline {
                                pipeline_id.set(Some(pid));
                                pitch_failed.set(false);
                                pitch_stage.set("Analyse en cours…".to_string());
                                pitch_loading.set(true);
                                pitch_poll_tick.update(|t| *t += 1);
                            }
                        });
                        if !show_unlock.get_untracked() && !show_pitch.get_untracked() {
                            toast_msg.set(Some("✓ Email confirmé — conversation déverrouillée.".to_string()));
                            spawn_local(async move {
                                sleep_ms(5000).await;
                                toast_msg.set(None);
                            });
                        }
                    }
                    _ => email_poll_tick.update(|t| *t += 1),
                },
                Err(_) => email_poll_tick.update(|t| *t += 1),
            }
        });
    });

    let reset_textarea_height = move || {
        let _ = js_sys::Function::new_no_args(
            "var t=document.querySelector('.chat-textarea');\
             if(t){t.style.height='auto';}"
        ).call0(&wasm_bindgen::JsValue::NULL);
    };

    let send = move || {
        let msg = input_text.get_untracked().trim().to_string();
        if msg.is_empty() || is_loading.get_untracked() { return; }

        let err_msg     = t(lang.get_untracked(), "chat.error").to_string();
        let offline_msg = t(lang.get_untracked(), "chat.offline").to_string();

        let history: Vec<HistoryMsg> = messages.get_untracked()
            .into_iter()
            .map(|(is_user, raw, _)| HistoryMsg {
                role: if is_user { "user" } else { "assistant" }.to_string(),
                content: raw,
            })
            .collect();

        let msg_for_api = msg.clone();
        let msg_restore = msg.clone();
        let sid = session_id_send.clone();
        reset_textarea_height();
        batch(move || {
            input_text.set(String::new());
            messages.update(|v| v.push((true, msg.clone(), msg.clone())));
            pending_options.set(vec![]);
            is_loading.set(true);
        });

        spawn_local(async move {
            let fp = compute_fingerprint();
            let result = Request::post("/api/ai/chat")
                .json(&ChatRequest { description: msg_for_api, history, session_id: sid, fingerprint: Some(fp) })
                .unwrap()
                .send()
                .await;

            match result {
                Ok(resp) => {
                    if resp.status() == 402 {
                        // Undo optimistic update — restore text to textarea, remove from chat
                        messages.update(|v| { v.pop(); });
                        batch(move || {
                            input_text.set(msg_restore);
                            unlock_for_pipeline.set(false);
                            show_unlock.set(true);
                            is_loading.set(false);
                        });
                        return;
                    }
                    match resp.json::<ChatResponse>().await {
                        Ok(data) => {
                            let (text, pitch, opts) = parse_response(&data.response);
                            // Also check for options in the dedicated field (backend-parsed)
                            let final_opts = if !data.options.is_empty() { data.options } else { opts };
                            let html = render_markdown(&text);
                            batch(move || {
                                messages.update(|v| v.push((false, text, html)));
                                pending_options.set(final_opts);
                                if let Some(p) = pitch {
                                    current_pitch.set(Some(p));
                                    pitch_page.set(0);
                                }
                                messages_used.set(data.messages_used);
                                if let Some(pid) = data.pipeline_id {
                                    pipeline_id.set(Some(pid));
                                    // New qualification → drop any previously shown
                                    // proposal so we display loading, not the old one.
                                    current_pitch.set(None);
                                    pitch_failed.set(false);
                                    pitch_stage.set("Analyse en cours…".to_string());
                                    pitch_loading.set(true);
                                    pitch_poll_tick.update(|t| *t += 1);
                                }
                                // Qualified but not unlocked: collect the email
                                // before the pipeline runs (the backend stashed
                                // the qualification and will spawn on confirm).
                                if data.needs_unlock {
                                    unlock_for_pipeline.set(true);
                                    show_unlock.set(true);
                                }
                                is_loading.set(false);
                            });
                        }
                        Err(_) => {
                            let html = render_markdown(&err_msg);
                            batch(move || {
                                messages.update(|v| v.push((false, err_msg, html)));
                                is_loading.set(false);
                            });
                        }
                    }
                },
                Err(_) => {
                    let html = render_markdown(&offline_msg);
                    batch(move || {
                        messages.update(|v| v.push((false, offline_msg, html)));
                        is_loading.set(false);
                    });
                }
            }
        });
    };

    let on_unlock_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let email = email_input.get_untracked().trim().to_string();
        if email.is_empty() { return; }
        let sid = session_id_unlock.clone();
        spawn_local(async move {
            let resp = Request::post("/api/auth/unlock")
                .json(&UnlockRequest { session_id: sid, email })
                .unwrap()
                .send()
                .await;
            match resp {
                Ok(r) => {
                    match r.json::<UnlockResponse>().await {
                        Ok(data) if data.ok => {
                            batch(move || {
                                unlock_error.set(false);
                                email_pending.set(true);
                                email_poll_tick.update(|t| *t += 1);
                            });
                        }
                        _ => unlock_error.set(true),
                    }
                }
                Err(_) => unlock_error.set(true),
            }
        });
    };

    let on_unlock_submit = store_value(on_unlock_submit);

    let send_clone = send.clone();
    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            send_clone();
        }
    };

    // Cloned for the selectable-options block (cloned again per button inside).
    let send_for_options = send.clone();

    view! {
        <div class="relative flex flex-col bg-deep" style="height: calc(100vh - 65px);">

            {/* Email unlock modal */}
            {move || show_unlock.get().then(|| view! {
                <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm animate-overlay-in">
                    <div class="glass rounded-2xl p-8 shadow-card animate-modal-enter" style="width: min(440px, calc(100vw - 3rem))">
                        {/* Header row */}
                        <div class="flex items-start justify-between mb-3">
                            <p class="eyebrow">"pointe.dev"</p>
                            <button
                                on:click=move |_| show_unlock.set(false)
                                class="pitch-close"
                                style="position: static; width: 28px; height: 28px; font-size: 14px;"
                                aria-label="Fermer"
                            >"✕"</button>
                        </div>

                        {move || if email_pending.get() {
                            // ── Pending / confirmed state ──────────────────
                            view! {
                                <div class="text-center py-2">
                                    {move || if email_confirmed.get() {
                                        view! {
                                            <div class="w-14 h-14 rounded-full flex items-center justify-center mx-auto mb-5" style="background:rgba(74,222,128,0.1)">
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-7 h-7" style="color:#4ade80" stroke-linecap="round" stroke-linejoin="round">
                                                    <polyline points="20 6 9 17 4 12"/>
                                                </svg>
                                            </div>
                                            <h3 class="text-lg font-bold text-primary mb-2">"✓ Email confirmé"</h3>
                                            <p class="text-sm text-secondary leading-relaxed mb-6">
                                                "Votre conversation est maintenant déverrouillée."
                                            </p>
                                            <button
                                                on:click=move |_| show_unlock.set(false)
                                                class="btn-primary w-full"
                                            >"Continuer →"</button>
                                        }.into_view()
                                    } else {
                                        view! {
                                            <div class="w-14 h-14 rounded-full bg-red-500/10 flex items-center justify-center mx-auto mb-5">
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-7 h-7 text-red-400" stroke-linecap="round" stroke-linejoin="round">
                                                    <rect x="2" y="4" width="20" height="16" rx="2"/>
                                                    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>
                                                </svg>
                                            </div>
                                            <h3 class="text-lg font-bold text-primary mb-2">"Vérifiez votre email"</h3>
                                            <p class="text-sm text-secondary leading-relaxed">
                                                "Un lien de confirmation a été envoyé à "
                                                <strong class="text-primary">{move || email_input.get()}</strong>
                                                "."
                                            </p>
                                            <div class="flex items-center justify-center gap-2 mt-3">
                                                <span class="pitch-pending-dot"></span>
                                                <p class="text-xs text-muted">"En attente de validation…"</p>
                                            </div>
                                            <button
                                                on:click=move |_| email_pending.set(false)
                                                class="text-xs text-red-400 hover:text-red-300 transition-colors mt-5"
                                            >"Changer d'adresse →"</button>
                                        }.into_view()
                                    }}
                                </div>
                            }.into_view()
                        } else {
                            // ── Email form ────────────────────────────────
                            view! {
                                <div>
                                    <h3 class="text-lg font-bold text-primary mb-2">
                                        {move || if unlock_for_pipeline.get() {
                                            "Lançons votre analyse"
                                        } else {
                                            "Continuez gratuitement"
                                        }}
                                    </h3>
                                    <p class="text-sm text-secondary mb-6 leading-relaxed">
                                        {move || if unlock_for_pipeline.get() {
                                            view! {
                                                "Confirmez votre email et nous construisons votre solution sur mesure — vous recevrez la proposition par email. Sans spam, promis."
                                            }.into_view()
                                        } else {
                                            view! {
                                                "Vous avez utilisé vos " {FREE_MESSAGES} " messages gratuits. Entrez votre email pour continuer — sans spam, promis."
                                            }.into_view()
                                        }}
                                    </p>
                                    <form on:submit=move |ev| on_unlock_submit.with_value(|f| f(ev)) class="flex flex-col gap-3">
                                        <input
                                            type="email"
                                            placeholder="votre@email.com"
                                            class="w-full px-4 py-3 rounded-xl border border-subtle bg-elevated text-sm text-primary placeholder-gray-600 focus:outline-none focus:border-red-500 transition-colors"
                                            prop:value=move || email_input.get()
                                            on:input=move |ev| email_input.set(event_target_value(&ev))
                                        />
                                        {move || unlock_error.get().then(|| view! {
                                            <p class="text-xs text-red-400">"Email invalide ou erreur réseau, réessayez."</p>
                                        })}
                                        <button type="submit" class="btn-primary w-full">
                                            "Envoyer le lien →"
                                        </button>
                                    </form>
                                </div>
                            }.into_view()
                        }}
                    </div>
                </div>
            })}

            {/* Pitch modal */}
            {move || show_pitch.get().then(|| view! {
                <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md">
                    <div class="pitch-card" style="pointer-events: auto;">
                        <button class="pitch-close" aria-label="Fermer" on:click=move |_| show_pitch.set(false)>"✕"</button>

                        {move || if pitch_loading.get() {
                            // ── Loading state ──────────────────────────────
                            view! {
                                <div class="pitch-loading-state">
                                    <div class="pitch-loading-spinner"></div>
                                    <p class="pitch-loading-title">"Votre projet est en cours d'analyse…"</p>
                                    <p class="pitch-loading-sub">"Notre équipe IA examine votre besoin — résultat dans quelques instants."</p>
                                </div>
                            }.into_view()
                        } else {
                            let pitch = current_pitch.get().unwrap_or_else(|| PitchData { slides: vec![] });
                            let total = pitch.slides.len();
                            view! {
                                <div>
                                    {/* Slide content */}
                                    {move || {
                                        let pitch2 = current_pitch.get().unwrap_or_else(|| PitchData { slides: vec![] });
                                        let total2 = pitch2.slides.len();
                                        let page = pitch_page.get().min(total2.saturating_sub(1));
                                        let slide = pitch2.slides.get(page).cloned().unwrap_or_else(|| PitchSlide {
                                            title: String::new(), body: String::new(), points: vec![],
                                        });
                                        view! {
                                            <div class="pitch-slide">
                                                <div class="pitch-page-indicator">
                                                    {(0..total2).map(|i| view! {
                                                        <span class=move || if pitch_page.get() == i { "pitch-dot pitch-dot-active" } else { "pitch-dot" }></span>
                                                    }).collect_view()}
                                                </div>
                                                <h2 class="pitch-title">{slide.title.clone()}</h2>
                                                <p class="pitch-body">{slide.body.clone()}</p>
                                                {(!slide.points.is_empty()).then(|| view! {
                                                    <ul class="pitch-points">
                                                        {slide.points.iter().map(|p| view! {
                                                            <li class="pitch-point">
                                                                <span class="pitch-point-dot"></span>
                                                                {p.clone()}
                                                            </li>
                                                        }).collect_view()}
                                                    </ul>
                                                })}
                                            </div>
                                        }
                                    }}

                                    {/* Navigation */}
                                    <div class="pitch-nav">
                                        <button
                                            class=move || if pitch_page.get() == 0 { "pitch-nav-btn pitch-nav-btn-ghost" } else { "pitch-nav-btn" }
                                            disabled=move || pitch_page.get() == 0
                                            aria-label="Diapositive précédente"
                                            on:click=move |_| pitch_page.update(|p| *p = p.saturating_sub(1))
                                        >"←"</button>
                                        <span class="pitch-nav-count">
                                            {move || pitch_page.get() + 1}" / "{total}
                                        </span>
                                        {move || if pitch_page.get() + 1 < total {
                                            view! {
                                                <button class="pitch-nav-btn"
                                                    aria-label="Diapositive suivante"
                                                    on:click=move |_| pitch_page.update(|p| *p += 1)
                                                >"→"</button>
                                            }.into_view()
                                        } else { view! { <span></span> }.into_view() }}
                                    </div>

                                    {/* CTA section — normal vs manual_quote */}
                                    <div class="pitch-quote-section">
                                        {move || if pitch_manual_quote.get() {
                                            // ── Manual quote CTA ──────────────────────────────
                                            view! {
                                                <div class="pitch-manual-banner">
                                                    <p class="pitch-manual-title">"⏱ Devis personnalisé sous 24h"</p>
                                                    <p class="pitch-manual-sub">"Notre équipe analyse votre projet et vous revient avec une estimation détaillée."</p>
                                                </div>
                                                {move || if email_confirmed.get() || is_unlocked.get() {
                                                    view! {
                                                        <p class="pitch-email-confirmed">"✓ Vous serez contacté sous 24h."</p>
                                                    }.into_view()
                                                } else if email_pending.get() {
                                                    view! {
                                                        <div class="pitch-email-pending">
                                                            <span class="pitch-pending-dot"></span>
                                                            "En attente de validation mail…"
                                                            <br/>
                                                            <span class="text-xs text-muted">
                                                                "Cliquez sur le lien envoyé à "
                                                                <strong>{move || email_input.get()}</strong>
                                                            </span>
                                                            <br/>
                                                            <button
                                                                class="text-xs text-red-400 hover:text-red-300 mt-2"
                                                                on:click=move |_| {
                                                                    email_pending.set(false);
                                                                    email_confirmed.set(false);
                                                                }
                                                            >"Changer d'adresse →"</button>
                                                        </div>
                                                    }.into_view()
                                                } else {
                                                    view! {
                                                        <div class="pitch-quote-form">
                                                            <input
                                                                type="email"
                                                                placeholder="votre@email.com"
                                                                class="pitch-quote-input"
                                                                prop:value=move || email_input.get()
                                                                on:input=move |ev| email_input.set(event_target_value(&ev))
                                                            />
                                                            <button
                                                                class="btn-primary w-full"
                                                                on:click=move |_| {
                                                                    let email = email_input.get_untracked().trim().to_string();
                                                                    if email.is_empty() { return; }
                                                                    let sid = session_id_pitch_form.get_value();
                                                                    spawn_local(async move {
                                                                        let resp = Request::post("/api/auth/unlock")
                                                                            .json(&UnlockRequest { session_id: sid, email })
                                                                            .unwrap().send().await;
                                                                        if let Ok(r) = resp {
                                                                            if r.json::<UnlockResponse>().await.ok().map(|d| d.ok).unwrap_or(false) {
                                                                                batch(move || {
                                                                                    email_pending.set(true);
                                                                                    email_poll_tick.update(|t| *t += 1);
                                                                                });
                                                                            }
                                                                        }
                                                                    });
                                                                }
                                                            >"Être contacté →"</button>
                                                        </div>
                                                    }.into_view()
                                                }}
                                            }.into_view()
                                        } else {
                                            // ── Normal quote CTA ──────────────────────────────
                                            view! {
                                                <div>
                                                {/* Price card */}
                                                {move || {
                                                    let cents = pitch_price_cents.get();
                                                    (cents > 0).then(|| {
                                                        let euros = cents / 100;
                                                        let price_str = if euros >= 1000 {
                                                            format!("{} {:03} €", euros / 1000, euros % 1000)
                                                        } else {
                                                            format!("{} €", euros)
                                                        };
                                                        let validity = pitch_price_validity.get();
                                                        view! {
                                                            <div class="pitch-price-card">
                                                                <span class="pitch-price-amount">{price_str}</span>
                                                                {(!validity.is_empty()).then(|| view! {
                                                                    <span class="pitch-price-validity">{validity}</span>
                                                                })}
                                                            </div>
                                                        }
                                                    })
                                                }}
                                                <p class="pitch-quote-sent">"✓ Cette proposition vous a été envoyée par email."</p>
                                                {move || pipeline_id.get().map(|pid| view! {
                                                    <button
                                                        class="btn-primary w-full pitch-checkout-btn"
                                                        prop:disabled=move || checkout_pending.get()
                                                        on:click=move |_| {
                                                            if checkout_pending.get_untracked() { return; }
                                                            let pid = pid.clone();
                                                            batch(move || { checkout_pending.set(true); checkout_error.set(false); });
                                                            spawn_local(async move {
                                                                let resp = Request::post("/api/stripe/checkout")
                                                                    .json(&CheckoutRequest { pipeline_id: pid }).unwrap()
                                                                    .send().await;
                                                                match resp {
                                                                    Ok(r) if r.ok() => {
                                                                        if let Ok(d) = r.json::<CheckoutResponse>().await {
                                                                            let _ = web_sys::window()
                                                                                .and_then(|w| w.location().set_href(&d.checkout_url).ok());
                                                                            return;
                                                                        }
                                                                        batch(move || { checkout_error.set(true); checkout_pending.set(false); });
                                                                    }
                                                                    _ => batch(move || { checkout_error.set(true); checkout_pending.set(false); }),
                                                                }
                                                            });
                                                        }
                                                    >
                                                        {move || if checkout_pending.get() { "Redirection…" } else { "Démarrer le projet →" }}
                                                    </button>
                                                })}
                                                {move || checkout_error.get().then(|| view! {
                                                    <p class="text-xs text-red-400 mt-2">"Paiement momentanément indisponible — réessayez ou contactez-nous."</p>
                                                })}
                                                </div>
                                            }.into_view()
                                        }}
                                    </div>
                                </div>
                            }.into_view()
                        }}
                    </div>
                </div>
            })}

            {/* Chat */}
            <div class="flex flex-col flex-1 min-w-0 overflow-hidden">

                {/* Header */}
                <div class="border-b border-subtle px-6 py-6 shrink-0">
                    <div class="max-w-3xl mx-auto flex items-start justify-between">
                        <div>
                            <h2 class="text-2xl font-bold text-primary">
                                {move || t(lang.get(), "chat.title")}
                            </h2>
                            <p class="text-base text-muted mt-1 font-light">
                                {move || t(lang.get(), "chat.sub")}
                            </p>
                        </div>
                        <div class="flex flex-col items-end gap-2 shrink-0 mt-1">
                            {move || {
                                let used = messages_used.get();
                                let remaining = FREE_MESSAGES.saturating_sub(used);
                                (used > 0 && !is_unlocked.get()).then(|| view! {
                                    <span class=move || format!(
                                        "text-xs px-2.5 py-1 rounded-full border border-subtle text-muted {}",
                                        if remaining == 0 { "chat-quota-empty" }
                                        else if remaining > 1 { "opacity-60" }
                                        else { "" }
                                    )>
                                        {if remaining == 0 {
                                            "Quota atteint".to_string()
                                        } else if remaining == 1 {
                                            "Dernier message gratuit".to_string()
                                        } else {
                                            format!("{remaining} messages gratuits")
                                        }}
                                    </span>
                                })
                            }}
                            {move || {
                                let show = !pitch_failed.get()
                                    && (current_pitch.get().is_some()
                                        || pitch_loading.get()
                                        || pitch_manual_quote.get());
                                show.then(|| view! {
                                    <button
                                        class=move || if pitch_loading.get() {
                                            "pitch-trigger-btn pitch-trigger-loading"
                                        } else { "pitch-trigger-btn pitch-trigger-ready" }
                                        on:click=move |_| { pitch_page.set(0); show_pitch.set(true); }
                                    >
                                        {move || if pitch_loading.get() {
                                            format!("⏳ {}", pitch_stage.get())
                                        } else if pitch_manual_quote.get() {
                                            "📋 Voir la proposition".to_string()
                                        } else {
                                            "✨ Voir notre proposition".to_string()
                                        }}
                                    </button>
                                })
                            }}
                            {move || pitch_failed.get().then(|| view! {
                                <div class="pitch-failed-note">
                                    "⚠️ Un souci technique a interrompu la préparation de votre proposition. Notre équipe a été notifiée et reviendra vers vous rapidement."
                                </div>
                            })}
                        </div>
                    </div>
                </div>

                {/* Messages */}
                <div class="chat-scroll flex-1 overflow-y-auto px-6 py-6">
                    <div class="max-w-3xl mx-auto space-y-5">
                        {/* Welcome — empty-state only, not part of the saved conversation */}
                        {move || messages.get().is_empty().then(|| view! {
                            <div class="flex justify-start">
                                <div class="chat-md max-w-[80%] px-5 py-4 glass text-secondary rounded-2xl rounded-tl-sm text-base leading-relaxed">
                                    {move || t(lang.get(), "chat.welcome")}
                                </div>
                            </div>
                        })}
                        {move || {
                            messages.get().into_iter().enumerate().map(|(i, (is_user, raw, html))| {
                                let (outer, inner) = if is_user {
                                    (
                                        "flex justify-end flex-col items-end gap-1",
                                        "max-w-[80%] px-5 py-4 bg-red-600 text-white rounded-2xl rounded-tr-sm text-base leading-relaxed",
                                    )
                                } else {
                                    (
                                        "flex justify-start flex-col items-start gap-1",
                                        "chat-md max-w-[80%] px-5 py-4 glass text-secondary rounded-2xl rounded-tl-sm text-base leading-relaxed",
                                    )
                                };
                                view! {
                                    <div class=outer>
                                        <div class=inner inner_html=html></div>
                                        {(!is_user).then(|| {
                                            let text = raw.clone();
                                            view! {
                                                <button
                                                    on:click=move |_| {
                                                        copy_text(&text);
                                                        copied_idx.set(Some(i));
                                                        let ci = copied_idx;
                                                        let cb = Closure::<dyn Fn()>::wrap(Box::new(move || {
                                                            let _ = ci.try_update(|v| *v = None);
                                                        }));
                                                        if let Some(w) = web_sys::window() {
                                                            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                                                cb.as_ref().unchecked_ref(), 2000
                                                            );
                                                        }
                                                        cb.forget();
                                                    }
                                                    class="copy-btn"
                                                    title="Copier"
                                                >
                                                    {move || if copied_idx.get() == Some(i) {
                                                        view! {
                                                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                                <polyline points="2.5 8.5 6 12 13.5 4"/>
                                                            </svg>
                                                        }.into_view()
                                                    } else {
                                                        view! {
                                                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                                                                <rect x="5" y="1" width="9" height="9" rx="1.5"/>
                                                                <rect x="1" y="5" width="9" height="9" rx="1.5"/>
                                                            </svg>
                                                        }.into_view()
                                                    }}
                                                </button>
                                            }
                                        })}
                                    </div>
                                }
                            }).collect_view()
                        }}

                        {/* Selectable options — shown below last assistant message */}
                        {move || {
                            let opts = pending_options.get();
                            if opts.is_empty() || is_loading.get() { return None; }
                            Some(view! {
                                <div class="flex justify-start">
                                    <div class="max-w-[80%] flex flex-col gap-2 pt-1">
                                        <div class="flex flex-wrap gap-2">
                                            {opts.into_iter().map(|opt| {
                                                let label = opt.label.clone();
                                                let label2 = label.clone();
                                                let send = send_for_options.clone();
                                                view! {
                                                    <button
                                                        class="chat-option-btn"
                                                        on:click=move |_| {
                                                            input_text.set(label2.clone());
                                                            send();
                                                        }
                                                    >
                                                        {label}
                                                    </button>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </div>
                                </div>
                            })
                        }}

                        {move || is_loading.get().then(|| view! {
                            <div class="flex justify-start">
                                <div class="px-5 py-3 glass rounded-2xl rounded-tl-sm">
                                    <div class="flex gap-1.5 items-center h-4">
                                        <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay:0ms"></span>
                                        <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay:140ms"></span>
                                        <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay:280ms"></span>
                                    </div>
                                </div>
                            </div>
                        })}

                        <div id="chat-end"></div>
                    </div>
                </div>

                {/* Continue on messaging apps — only shown after unlock */}
                {move || is_unlocked.get().then(|| view! {
                    <div class="px-6 py-2 border-t border-subtle">
                        <div class="max-w-3xl mx-auto flex items-center gap-3">
                            <span class="text-xs text-muted">{move || t(lang.get(), "chat.continueOn")}</span>
                            <a
                                href="https://wa.me/33600000000"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="text-xs px-2.5 py-1 rounded-full border border-subtle text-muted hover:border-green-500 hover:text-green-400 transition-colors"
                            >
                                "WhatsApp"
                            </a>
                            <a
                                href="https://t.me/pointedev"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="text-xs px-2.5 py-1 rounded-full border border-subtle text-muted hover:border-sky-500 hover:text-sky-400 transition-colors"
                            >
                                "Telegram"
                            </a>
                        </div>
                    </div>
                })}

                {/* Input */}
                <div class="border-t border-subtle px-6 py-4">
                    <div class="max-w-3xl mx-auto flex gap-3 items-center">
                        <textarea
                            class="chat-textarea flex-1 bg-elevated border border-subtle rounded-xl px-4 py-3 text-base text-primary placeholder-gray-600 focus:outline-none focus:border-red-600 transition-colors leading-relaxed"
                            placeholder=move || t(lang.get(), "chat.placeholder")
                            rows="1"
                            prop:value=move || input_text.get()
                            on:input=move |ev| {
                                input_text.set(event_target_value(&ev));
                                if let Some(target) = ev.target() {
                                    let _ = js_sys::Function::new_with_args(
                                        "t",
                                        "t.style.height='auto';t.style.height=t.scrollHeight+'px';"
                                    ).call1(&wasm_bindgen::JsValue::NULL, &target);
                                }
                            }
                            on:keydown=on_keydown
                        ></textarea>
                        <button
                            on:click=move |_| send()
                            class="chat-send-btn shrink-0"
                            disabled=move || is_loading.get()
                            aria-label="Envoyer"
                        >
                            <svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M8 13V3M8 3L3.5 7.5M8 3L12.5 7.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
        </div>

        {/* Toast notification */}
        {move || toast_msg.get().map(|msg| view! {
            <div class="chat-toast">{msg}</div>
        })}

    }
}
