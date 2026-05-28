use leptos::*;
use serde::{Deserialize, Serialize};
use leptos::spawn_local;
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use crate::i18n::{Lang, t};
use crate::components::workflow_canvas::{WorkflowCanvas, WorkflowGraph};

const FREE_MESSAGES: u32 = 5;

fn get_or_create_session_id() -> String {
    let window = web_sys::window().unwrap();
    let storage = window.local_storage().unwrap().unwrap();
    if let Ok(Some(id)) = storage.get_item("_sid") {
        return id;
    }
    // Generate a simple UUID-like ID via Math.random
    let id = js_sys::eval(
        "'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g,\
         function(c){var r=Math.random()*16|0,v=c=='x'?r:(r&0x3|0x8);return v.toString(16)})"
    ).ok()
     .and_then(|v| v.as_string())
     .unwrap_or_else(|| format!("{}", js_sys::Date::now() as u64));
    let _ = storage.set_item("_sid", &id);
    id
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
}

#[derive(Deserialize)]
struct ChatResponse {
    response: String,
    messages_used: u32,
    messages_free: u32,
    /// Present when the AI qualified the prospect and launched a pipeline.
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
}

// Extract ```workflow JSON blocks from AI response
fn parse_workflow(content: &str) -> (String, Option<WorkflowGraph>) {
    const OPEN: &str = "```workflow";
    const CLOSE: &str = "\n```";
    if let Some(s) = content.find(OPEN) {
        let after_tag = &content[s + OPEN.len()..];
        let after = if let Some(nl) = after_tag.find('\n') {
            &after_tag[nl + 1..]
        } else {
            return (content.to_string(), None);
        };
        if let Some(e) = after.find(CLOSE) {
            let json = after[..e].trim();
            let before = content[..s].trim();
            let rest   = after[e + CLOSE.len()..].trim();
            let text = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{}\n\n{}", before, rest),
            };
            let graph = serde_json::from_str::<WorkflowGraph>(json).ok();
            return (text, graph);
        }
    }
    (content.to_string(), None)
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

    let session_id = get_or_create_session_id();
    let session_id_send = session_id.clone();
    let session_id_unlock = session_id.clone();

    let welcome_raw = t(lang.get_untracked(), "chat.welcome").to_string();
    let welcome_html = render_markdown(&welcome_raw);

    // (is_user, raw_text_for_copy, pre-rendered_html)
    let messages = create_rw_signal::<Vec<(bool, String, String)>>(vec![(false, welcome_raw, welcome_html)]);
    let input_text    = create_rw_signal(String::new());
    let is_loading    = create_rw_signal(false);
    let copied_idx: RwSignal<Option<usize>> = create_rw_signal(None);
    let current_graph: RwSignal<Option<WorkflowGraph>> = create_rw_signal(None);
    let messages_used: RwSignal<u32> = create_rw_signal(0);
    let show_unlock: RwSignal<bool> = create_rw_signal(false);
    let email_input = create_rw_signal(String::new());
    let unlock_error = create_rw_signal(false);
    let pipeline_id: RwSignal<Option<String>> = create_rw_signal(None);

    // Auto-scroll to end on new message
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

        // Capture history before adding current message (skip index 0 = welcome)
        let history: Vec<HistoryMsg> = messages.get_untracked()
            .into_iter()
            .skip(1)
            .map(|(is_user, raw, _)| HistoryMsg {
                role: if is_user { "user" } else { "assistant" }.to_string(),
                content: raw,
            })
            .collect();

        let msg_for_api = msg.clone();
        let sid = session_id_send.clone();
        batch(move || {
            input_text.set(String::new());
            messages.update(|v| v.push((true, msg.clone(), msg.clone())));
            is_loading.set(true);
        });

        spawn_local(async move {
            let result = Request::post("/api/ai/chat")
                .json(&ChatRequest { description: msg_for_api, history, session_id: sid })
                .unwrap()
                .send()
                .await;

            match result {
                Ok(resp) => {
                    if resp.status() == 402 {
                        batch(move || {
                            show_unlock.set(true);
                            is_loading.set(false);
                        });
                        return;
                    }
                    match resp.json::<ChatResponse>().await {
                        Ok(data) => {
                            let (text, graph) = parse_workflow(&data.response);
                            let html = render_markdown(&text);
                            batch(move || {
                                messages.update(|v| v.push((false, text, html)));
                                if let Some(g) = graph { current_graph.set(Some(g)); }
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

    // Email unlock submit
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
                            batch(move || {
                                show_unlock.set(false);
                                unlock_error.set(false);
                                messages_used.set(0);
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
        <div class="relative flex flex-col bg-white dark:bg-black" style="height: calc(100vh - 65px);">

            {/* Email unlock modal */}
            {move || show_unlock.get().then(|| view! {
                <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
                    <div class="bg-white dark:bg-gray-950 rounded-2xl border border-gray-200 dark:border-gray-800 p-8 max-w-sm w-full mx-4 shadow-2xl">
                        <p class="text-xs text-red-600 uppercase tracking-widest mb-3">"pointe.dev"</p>
                        <h3 class="text-lg font-bold text-gray-900 dark:text-white mb-2">
                            "Continuez gratuitement"
                        </h3>
                        <p class="text-sm text-gray-500 dark:text-gray-400 mb-6 leading-relaxed">
                            "Vous avez utilisé vos " {FREE_MESSAGES} " messages gratuits. Entrez votre email pour continuer — sans spam, promis."
                        </p>
                        <form on:submit=move |ev| on_unlock_submit.with_value(|f| f(ev)) class="flex flex-col gap-3">
                            <input
                                type="email"
                                placeholder="votre@email.com"
                                class="w-full px-4 py-3 rounded-xl border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 text-sm text-gray-900 dark:text-white placeholder-gray-400 focus:outline-none focus:border-red-500 transition-colors"
                                prop:value=move || email_input.get()
                                on:input=move |ev| email_input.set(event_target_value(&ev))
                            />
                            {move || unlock_error.get().then(|| view! {
                                <p class="text-xs text-red-500">"Email invalide, réessayez."</p>
                            })}
                            <button
                                type="submit"
                                class="w-full py-3 bg-red-600 hover:bg-red-700 text-white rounded-xl text-sm font-semibold transition-colors"
                            >
                                "Continuer la conversation →"
                            </button>
                        </form>
                    </div>
                </div>
            })}

            {/* Main: chat + canvas */}
            <div class="flex flex-1 overflow-hidden">

                {/* Chat column */}
                <div class="flex flex-col flex-1 min-w-0 overflow-hidden">

                    {/* Header */}
                    <div class="border-b border-gray-100 dark:border-gray-900 px-6 py-6 shrink-0">
                        <div class="max-w-2xl mx-auto flex items-start justify-between">
                            <div>
                                <p class="text-xs text-gray-400 dark:text-gray-600 uppercase tracking-widest mb-2">"pointe.dev"</p>
                                <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                                    {move || t(lang.get(), "chat.title")}
                                </h2>
                                <p class="text-sm text-gray-400 dark:text-gray-500 mt-1 font-light">
                                    {move || t(lang.get(), "chat.sub")}
                                </p>
                            </div>
                            <div class="flex flex-col items-end gap-2 shrink-0 mt-1">
                                {move || {
                                    let used = messages_used.get();
                                    let remaining = FREE_MESSAGES.saturating_sub(used);
                                    (used > 0).then(|| view! {
                                        <span class=move || format!(
                                            "text-xs px-2.5 py-1 rounded-full border {}",
                                            if remaining == 0 { "border-red-300 text-red-500 dark:border-red-800 dark:text-red-400" }
                                            else { "border-gray-200 text-gray-400 dark:border-gray-800 dark:text-gray-500" }
                                        )>
                                            {format!("{remaining} message{} gratuit{}", if remaining > 1 { "s" } else { "" }, if remaining > 1 { "s" } else { "" })}
                                        </span>
                                    })
                                }}
                                {move || pipeline_id.get().map(|_| view! {
                                    <span class="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full border border-red-200 text-red-500 dark:border-red-900 dark:text-red-400">
                                        <span class="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse"></span>
                                        "Workflow en cours d'analyse"
                                    </span>
                                })}
                            </div>
                        </div>
                    </div>

                    {/* Messages */}
                    <div class="chat-scroll flex-1 overflow-y-auto px-6 py-6">
                        <div class="max-w-2xl mx-auto space-y-5">
                            {move || {
                                messages.get().into_iter().enumerate().map(|(i, (is_user, raw, html))| {
                                    let (outer, inner) = if is_user {
                                        (
                                            "flex justify-end flex-col items-end gap-1",
                                            "max-w-[80%] px-5 py-3 bg-red-600 text-white rounded-2xl rounded-tr-sm text-sm leading-relaxed",
                                        )
                                    } else {
                                        (
                                            "flex justify-start flex-col items-start gap-1",
                                            "chat-md max-w-[80%] px-5 py-3 bg-gray-50 dark:bg-gray-950 text-gray-800 dark:text-gray-200 border border-gray-100 dark:border-gray-900 rounded-2xl rounded-tl-sm text-sm leading-relaxed",
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
                                                        class="text-base leading-none text-gray-300 hover:text-red-500 transition-colors pl-1"
                                                    >
                                                        {move || if copied_idx.get() == Some(i) { "✓" } else { "📋" }}
                                                    </button>
                                                }
                                            })}
                                        </div>
                                    }
                                }).collect_view()
                            }}

                            {move || is_loading.get().then(|| view! {
                                <div class="flex justify-start">
                                    <div class="px-5 py-3 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-2xl rounded-tl-sm">
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

                    {/* Continue on messaging apps */}
                    <div class="px-6 py-2 border-t border-gray-50 dark:border-gray-900/50">
                        <div class="max-w-2xl mx-auto flex items-center gap-3">
                        <span class="text-xs text-gray-300 dark:text-gray-700">"Continuer sur"</span>
                        <a
                            href="https://wa.me/33600000000"
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-xs px-2.5 py-1 rounded-full border border-gray-200 dark:border-gray-800 text-gray-400 hover:border-green-400 hover:text-green-600 dark:hover:text-green-400 transition-colors"
                        >
                            "WhatsApp"
                        </a>
                        <a
                            href="https://t.me/pointedev"
                            target="_blank"
                            rel="noopener noreferrer"
                            class="text-xs px-2.5 py-1 rounded-full border border-gray-200 dark:border-gray-800 text-gray-400 hover:border-sky-400 hover:text-sky-500 dark:hover:text-sky-400 transition-colors"
                        >
                            "Telegram"
                        </a>
                        </div>
                    </div>

                    {/* Input */}
                    <div class="border-t border-gray-100 dark:border-gray-900 px-6 py-4">
                        <div class="max-w-2xl mx-auto flex gap-3 items-center">
                            <textarea
                                class="flex-1 resize-none bg-gray-50 dark:bg-gray-950 border border-gray-200 dark:border-gray-800 rounded-xl px-4 py-3 text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-600 focus:outline-none focus:border-red-600 dark:focus:border-red-600 transition-colors leading-relaxed"
                                placeholder=move || t(lang.get(), "chat.placeholder")
                                rows="2"
                                prop:value=move || input_text.get()
                                on:input=move |ev| input_text.set(event_target_value(&ev))
                                on:keydown=on_keydown
                            ></textarea>
                            <button
                                on:click=move |_| send()
                                class="flex items-center justify-center px-5 py-3 bg-red-600 text-white rounded-xl hover:bg-red-700 transition-colors text-sm font-semibold shrink-0 disabled:opacity-40 disabled:cursor-not-allowed"
                                disabled=move || is_loading.get()
                            >
                                {move || t(lang.get(), "chat.send")}
                            </button>
                        </div>
                    </div>
                </div>

                {/* Workflow canvas — desktop only */}
                <div class="hidden lg:flex flex-col w-[480px] xl:w-[560px] border-l border-gray-100 dark:border-gray-900 bg-gray-50/30 dark:bg-gray-950/30 shrink-0">

                    {/* Canvas header */}
                    <div class="px-5 py-4 border-b border-gray-100 dark:border-gray-900 flex items-center justify-between shrink-0">
                        <p class="text-xs text-gray-400 uppercase tracking-widest">"Votre workflow"</p>
                        {move || current_graph.get().map(|_| view! {
                            <span class="flex items-center gap-1.5">
                                <span class="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse"></span>
                                <span class="text-xs text-gray-400">"Généré par IA"</span>
                            </span>
                        })}
                    </div>

                    {/* Canvas body */}
                    <div class="chat-scroll flex-1 overflow-y-auto p-6 relative">
                        {move || match current_graph.get() {
                            Some(g) => view! {
                                <div class="w-full animate-canvas-in">
                                    <WorkflowCanvas graph=g />
                                </div>
                            }.into_view(),
                            None => view! {
                                <div class="flex flex-col items-center justify-center h-full min-h-48 text-center px-4 py-8">
                                    <div class="w-16 h-16 rounded-2xl border-2 border-dashed border-gray-200 dark:border-gray-800 flex items-center justify-center mb-5">
                                        <svg class="w-7 h-7 text-gray-300 dark:text-gray-700" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 6v2.25a2.25 2.25 0 01-2.25 2.25H6a2.25 2.25 0 01-2.25-2.25V6zM3.75 15.75A2.25 2.25 0 016 13.5h2.25a2.25 2.25 0 012.25 2.25V18a2.25 2.25 0 01-2.25 2.25H6A2.25 2.25 0 013.75 18v-2.25zM13.5 6a2.25 2.25 0 012.25-2.25H18A2.25 2.25 0 0120.25 6v2.25A2.25 2.25 0 0118 10.5h-2.25a2.25 2.25 0 01-2.25-2.25V6zM13.5 15.75a2.25 2.25 0 012.25-2.25H18a2.25 2.25 0 012.25 2.25V18A2.25 2.25 0 0118 20.25h-2.25A2.25 2.25 0 0113.5 18v-2.25z" />
                                        </svg>
                                    </div>
                                    <p class="text-sm font-medium text-gray-400 dark:text-gray-600 mb-2">
                                        "Workflow en attente"
                                    </p>
                                    <p class="text-xs text-gray-300 dark:text-gray-700 leading-relaxed max-w-[160px]">
                                        "Décrivez votre processus — l'IA le visualisera ici en temps réel."
                                    </p>
                                </div>
                            }.into_view(),
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}
