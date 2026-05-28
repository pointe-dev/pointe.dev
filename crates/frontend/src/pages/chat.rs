use leptos::*;
use serde::{Deserialize, Serialize};
use leptos::spawn_local;
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use crate::i18n::{Lang, t};

#[derive(Serialize)]
struct HistoryMsg {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    description: String,
    history: Vec<HistoryMsg>,
}

#[derive(Deserialize)]
struct ChatResponse {
    response: String,
}

// Split off ```mermaid ... ``` blocks from AI response text
fn parse_mermaid(content: &str) -> (String, Option<String>) {
    const OPEN: &str = "```mermaid";
    const CLOSE: &str = "\n```";
    if let Some(s) = content.find(OPEN) {
        // skip the rest of the opening line (tolerates trailing spaces)
        let after_tag = &content[s + OPEN.len()..];
        let after = if let Some(nl) = after_tag.find('\n') {
            &after_tag[nl + 1..]
        } else {
            return (content.to_string(), None);
        };
        if let Some(e) = after.find(CLOSE) {
            let diagram = after[..e].trim().to_string();
            let before  = content[..s].trim();
            let rest    = after[e + CLOSE.len()..].trim();
            let text = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{}\n\n{}", before, rest),
            };
            return (text, Some(diagram));
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

    let welcome_raw = t(lang.get_untracked(), "chat.welcome").to_string();
    let welcome_html = render_markdown(&welcome_raw);

    // (is_user, raw_text_for_copy, pre-rendered_html)
    let messages = create_rw_signal::<Vec<(bool, String, String)>>(vec![(false, welcome_raw, welcome_html)]);
    let input_text    = create_rw_signal(String::new());
    let is_loading    = create_rw_signal(false);
    let copied_idx: RwSignal<Option<usize>> = create_rw_signal(None);
    let current_diagram: RwSignal<Option<String>> = create_rw_signal(None);

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

    // Render mermaid diagram when it changes
    create_effect(move |_| {
        if let Some(ref code) = current_diagram.get() {
            let _ = js_sys::Function::new_with_args(
                "code",
                "renderMermaidTo(code, 'mermaid-canvas')"
            ).call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::from_str(code));
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
        batch(move || {
            input_text.set(String::new());
            messages.update(|v| v.push((true, msg.clone(), msg.clone())));
            is_loading.set(true);
        });

        spawn_local(async move {
            let result = Request::post("/api/ai/chat")
                .json(&ChatRequest { description: msg_for_api, history })
                .unwrap()
                .send()
                .await;

            match result {
                Ok(resp) => match resp.json::<ChatResponse>().await {
                    Ok(data) => {
                        let (text, diagram) = parse_mermaid(&data.response);
                        let html = render_markdown(&text);
                        batch(move || {
                            messages.update(|v| v.push((false, text, html)));
                            if let Some(d) = diagram { current_diagram.set(Some(d)); }
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

    let send_clone = send.clone();
    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            send_clone();
        }
    };

    view! {
        <div class="flex flex-col bg-white dark:bg-black" style="min-height: calc(100vh - 65px);">

            {/* Main: chat + canvas */}
            <div class="flex flex-1 overflow-hidden">

                {/* Chat column */}
                <div class="flex flex-col flex-1 min-w-0 overflow-hidden">

                    {/* Header */}
                    <div class="border-b border-gray-100 dark:border-gray-900 px-6 py-6 shrink-0">
                        <div class="max-w-2xl mx-auto">
                            <p class="text-xs text-gray-400 dark:text-gray-600 uppercase tracking-widest mb-2">"pointe.dev"</p>
                            <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                                {move || t(lang.get(), "chat.title")}
                            </h2>
                            <p class="text-sm text-gray-400 dark:text-gray-500 mt-1 font-light">
                                {move || t(lang.get(), "chat.sub")}
                            </p>
                        </div>
                    </div>

                    {/* Messages */}
                    <div class="flex-1 overflow-y-auto px-6 py-6">
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

                {/* Mermaid canvas — desktop only */}
                <div class="hidden lg:flex flex-col w-[480px] xl:w-[560px] border-l border-gray-100 dark:border-gray-900 bg-gray-50/30 dark:bg-gray-950/30 shrink-0">

                    {/* Canvas header */}
                    <div class="px-5 py-4 border-b border-gray-100 dark:border-gray-900 flex items-center justify-between shrink-0">
                        <p class="text-xs text-gray-400 uppercase tracking-widest">"Votre workflow"</p>
                        {move || current_diagram.get().map(|_| view! {
                            <span class="flex items-center gap-1.5">
                                <span class="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse"></span>
                                <span class="text-xs text-gray-400">"Généré par IA"</span>
                            </span>
                        })}
                    </div>

                    {/* Canvas body */}
                    <div class="flex-1 overflow-y-auto p-6 relative">

                        {/* Mermaid SVG container — always in DOM so JS can inject */}
                        <div
                            id="mermaid-canvas"
                            class=move || if current_diagram.get().is_some() {
                                "w-full animate-canvas-in"
                            } else {
                                "hidden"
                            }
                        ></div>

                        {/* Empty state */}
                        {move || current_diagram.get().is_none().then(|| view! {
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
                        })}
                    </div>
                </div>
            </div>
        </div>
    }
}
