use leptos::*;
use serde::{Deserialize, Serialize};
use leptos::spawn_local;
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use crate::i18n::{Lang, t};

const FREE_MESSAGES: u32 = 5;

/// Returns (session_id, is_token_session).
/// If `_sid` is a 64-char hex token the user is already authenticated.
fn get_or_create_session_id() -> (String, bool) {
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
}

#[derive(Serialize)]
struct UnlockRequest {
    session_id: String,
    email: String,
}

#[derive(Deserialize)]
struct UnlockResponse {
    ok: bool,
    token: Option<String>,
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

fn parse_response(content: &str) -> (String, Option<PitchData>) {
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

    // Strip workflow block from remaining text (kept for future use, display suppressed)
    let display_text = {
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

    (display_text, pitch_json)
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

    let messages = create_rw_signal::<Vec<(bool, String, String)>>(vec![]);
    let input_text    = create_rw_signal(String::new());
    let is_loading    = create_rw_signal(false);
    let copied_idx: RwSignal<Option<usize>> = create_rw_signal(None);
    let current_pitch: RwSignal<Option<PitchData>> = create_rw_signal(None);
    let messages_used: RwSignal<u32> = create_rw_signal(0);
    let show_unlock: RwSignal<bool> = create_rw_signal(false);
    let email_input = create_rw_signal(String::new());
    let unlock_error = create_rw_signal(false);
    let pipeline_id: RwSignal<Option<String>> = create_rw_signal(None);
    let show_pitch: RwSignal<bool> = create_rw_signal(false);
    let pitch_page: RwSignal<usize> = create_rw_signal(0);

    create_effect(move |_| {
        let _ = messages.get();
        if let Some(w) = web_sys::window() {
            if let Some(doc) = w.document() {
                if let Some(el) = doc.get_element_by_id("chat-end") {
                    el.scroll_into_view();
                }
            }
        }
    });

    let send = move || {
        let msg = input_text.get_untracked().trim().to_string();
        if msg.is_empty() || is_loading.get_untracked() { return; }

        let err_msg     = t(lang.get_untracked(), "chat.error").to_string();
        let offline_msg = t(lang.get_untracked(), "chat.offline").to_string();

        let history: Vec<HistoryMsg> = messages.get_untracked()
            .into_iter()
            .skip(1)
            .map(|(is_user, raw, _)| HistoryMsg {
                role: if is_user { "user" } else { "assistant" }.to_string(),
                content: raw,
            })
            .collect();

        let msg_for_api = msg.clone();
        let msg_restore = msg.clone();
        let sid = session_id_send.clone();
        batch(move || {
            input_text.set(String::new());
            messages.update(|v| v.push((true, msg.clone(), msg.clone())));
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
                            show_unlock.set(true);
                            is_loading.set(false);
                        });
                        return;
                    }
                    match resp.json::<ChatResponse>().await {
                        Ok(data) => {
                            let (text, pitch) = parse_response(&data.response);
                            let html = render_markdown(&text);
                            batch(move || {
                                messages.update(|v| v.push((false, text, html)));
                                if let Some(p) = pitch {
                                    current_pitch.set(Some(p));
                                    pitch_page.set(0);
                                }
                                messages_used.set(data.messages_used);
                                if let Some(pid) = data.pipeline_id { pipeline_id.set(Some(pid)); }
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
                    if let Ok(data) = r.json::<UnlockResponse>().await {
                        if data.ok {
                            // Persist signed token so next visit skips email gate
                            if let Some(token) = data.token {
                                if let Some(w) = web_sys::window() {
                                    if let Ok(Some(storage)) = w.local_storage() {
                                        let _ = storage.set_item("_sid", &token);
                                    }
                                }
                            }
                            batch(move || {
                                show_unlock.set(false);
                                unlock_error.set(false);
                                messages_used.set(0);
                                is_unlocked.set(true);
                            });
                        } else {
                            unlock_error.set(true);
                        }
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

    view! {
        <div class="relative flex flex-col bg-deep" style="height: calc(100vh - 65px);">

            {/* Email unlock modal */}
            {move || show_unlock.get().then(|| view! {
                <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm animate-overlay-in">
                    <div class="glass rounded-2xl p-8 shadow-card animate-modal-enter" style="width: min(440px, calc(100vw - 3rem))">
                        {/* Close */}
                        <div class="flex items-start justify-between mb-3">
                            <p class="eyebrow">"pointe.dev"</p>
                            <button
                                on:click=move |_| show_unlock.set(false)
                                class="pitch-close"
                                style="position: static; width: 28px; height: 28px; font-size: 14px;"
                                aria-label="Fermer"
                            >"✕"</button>
                        </div>
                        <h3 class="text-lg font-bold text-primary mb-2">
                            "Continuez gratuitement"
                        </h3>
                        <p class="text-sm text-secondary mb-6 leading-relaxed">
                            "Vous avez utilisé vos " {FREE_MESSAGES} " messages gratuits. Entrez votre email pour continuer — sans spam, promis."
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
                                <p class="text-xs text-red-400">"Email invalide, réessayez."</p>
                            })}
                            <button type="submit" class="btn-primary w-full">
                                "Continuer la conversation →"
                            </button>
                        </form>
                    </div>
                </div>
            })}

            {/* Pitch modal */}
            {move || show_pitch.get().then(|| {
                let pitch = current_pitch.get().unwrap_or_else(|| PitchData { slides: vec![] });
                let total = pitch.slides.len();
                view! {
                    <div
                        class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md"
                        on:click=move |ev| {
                            if let Some(target) = ev.target() {
                                if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                                    if el.class_list().contains("pitch-backdrop") {
                                        show_pitch.set(false);
                                    }
                                }
                            }
                        }
                    >
                        <div class="pitch-card" style="pointer-events: auto;">

                            {/* Close */}
                            <button
                                class="pitch-close"
                                on:click=move |_| show_pitch.set(false)
                            >"✕"</button>

                            {/* Slide content */}
                            {move || {
                                let page = pitch_page.get().min(total.saturating_sub(1));
                                let slide = pitch.slides.get(page).cloned().unwrap_or_else(|| PitchSlide {
                                    title: String::new(),
                                    body: String::new(),
                                    points: vec![],
                                });
                                view! {
                                    <div class="pitch-slide">
                                        {/* Page indicator */}
                                        <div class="pitch-page-indicator">
                                            {(0..total).map(|i| view! {
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
                                    on:click=move |_| pitch_page.update(|p| *p = p.saturating_sub(1))
                                >"←"</button>

                                <span class="pitch-nav-count">
                                    {move || pitch_page.get() + 1}" / "{total}
                                </span>

                                {move || {
                                    let page = pitch_page.get();
                                    if page + 1 < total {
                                        view! {
                                            <button
                                                class="pitch-nav-btn"
                                                on:click=move |_| pitch_page.update(|p| *p += 1)
                                            >"→"</button>
                                        }.into_view()
                                    } else {
                                        view! {
                                            <button
                                                class="btn-primary btn-sm"
                                                on:click=move |_| show_pitch.set(false)
                                            >"Parlons-en →"</button>
                                        }.into_view()
                                    }
                                }}
                            </div>
                        </div>
                    </div>
                }
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
                                (used > 0).then(|| view! {
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
                            {move || current_pitch.get().map(|_| view! {
                                <button
                                    class="pitch-trigger-btn"
                                    on:click=move |_| { pitch_page.set(0); show_pitch.set(true); }
                                >
                                    <span class="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse shrink-0"></span>
                                    "Notre proposition"
                                </button>
                            })}
                        </div>
                    </div>
                </div>

                {/* Messages */}
                <div class="chat-scroll flex-1 overflow-y-auto px-6 py-6">
                    <div class="max-w-3xl mx-auto space-y-5">
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
                            <span class="text-xs text-muted">"Continuer sur"</span>
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
                            class="flex-1 resize-none bg-elevated border border-subtle rounded-xl px-4 py-3 text-base text-primary placeholder-gray-600 focus:outline-none focus:border-red-600 transition-colors leading-relaxed"
                            placeholder=move || t(lang.get(), "chat.placeholder")
                            rows="2"
                            prop:value=move || input_text.get()
                            on:input=move |ev| input_text.set(event_target_value(&ev))
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
    }
}
