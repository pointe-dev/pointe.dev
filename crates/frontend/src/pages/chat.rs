use leptos::*;
use serde::{Deserialize, Serialize};
use leptos::spawn_local;
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use crate::i18n::{Lang, t};

#[derive(Serialize)]
struct ChatRequest {
    description: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    response: String,
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

    let welcome_msg = move || t(lang.get(), "chat.welcome").to_string();

    let messages = create_rw_signal::<Vec<(bool, String)>>(vec![(
        false,
        welcome_msg(),
    )]);
    let input_text = create_rw_signal(String::new());
    let is_loading = create_rw_signal(false);
    let copied_idx: RwSignal<Option<usize>> = create_rw_signal(None);

    create_effect(move |_| {
        let _ = messages.get();
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id("chat-end") {
                    el.scroll_into_view();
                }
            }
        }
    });

    let send = move || {
        let msg = input_text.get_untracked().trim().to_string();
        if msg.is_empty() || is_loading.get_untracked() {
            return;
        }
        input_text.set(String::new());
        messages.update(|v| v.push((true, msg.clone())));
        is_loading.set(true);

        let err_msg = t(lang.get_untracked(), "chat.error").to_string();
        let offline_msg = t(lang.get_untracked(), "chat.offline").to_string();

        spawn_local(async move {
            let result = Request::post("/api/ai/chat")
                .json(&ChatRequest { description: msg })
                .unwrap()
                .send()
                .await;

            match result {
                Ok(resp) => match resp.json::<ChatResponse>().await {
                    Ok(data) => messages.update(|v| v.push((false, data.response))),
                    Err(_) => messages.update(|v| v.push((false, err_msg))),
                },
                Err(_) => messages.update(|v| v.push((false, offline_msg))),
            }
            is_loading.set(false);
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

            <div class="border-b border-gray-100 dark:border-gray-900 px-6 py-8 max-w-3xl mx-auto w-full">
                <p class="text-xs text-gray-400 dark:text-gray-600 uppercase tracking-widest mb-2">"pointe.dev"</p>
                <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                    {move || t(lang.get(), "chat.title")}
                </h2>
                <p class="text-sm text-gray-400 dark:text-gray-500 mt-1 font-light">
                    {move || t(lang.get(), "chat.sub")}
                </p>
            </div>

            <div class="flex-1 overflow-y-auto px-6 py-8 max-w-3xl mx-auto w-full">
                <div class="space-y-5">
                    {move || {
                        messages.get().into_iter().enumerate().map(|(i, (is_user, content))| {
                            let content_for_copy = content.clone();
                            let (outer, inner) = if is_user {
                                (
                                    "flex justify-end flex-col items-end gap-1",
                                    "max-w-[72%] px-5 py-3 bg-red-600 text-white rounded-2xl rounded-tr-sm text-sm leading-relaxed",
                                )
                            } else {
                                (
                                    "flex justify-start flex-col items-start gap-1",
                                    "max-w-[72%] px-5 py-3 bg-gray-50 dark:bg-gray-950 text-gray-800 dark:text-gray-200 border border-gray-100 dark:border-gray-900 rounded-2xl rounded-tl-sm text-sm leading-relaxed",
                                )
                            };
                            view! {
                                <div class=outer>
                                    <div class=inner>{content}</div>
                                    {(!is_user).then(|| {
                                        let text = content_for_copy.clone();
                                        view! {
                                            <button
                                                on:click=move |_| {
                                                    copy_text(&text);
                                                    copied_idx.set(Some(i));
                                                    let ci = copied_idx;
                                                    let cb = Closure::once(move || ci.set(None));
                                                    let _ = web_sys::window()
                                                        .unwrap()
                                                        .set_timeout_with_callback_and_timeout_and_arguments_0(
                                                            cb.as_ref().unchecked_ref(),
                                                            2000
                                                        );
                                                    cb.forget();
                                                }
                                                class="text-xs text-gray-400 hover:text-red-600 transition-colors pl-1"
                                            >
                                                {move || if copied_idx.get() == Some(i) {
                                                    t(lang.get(), "modal.copied")
                                                } else {
                                                    t(lang.get(), "modal.copy")
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
                            <div class="px-5 py-3 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-2xl rounded-tl-sm">
                                <div class="flex gap-1.5 items-center h-4">
                                    <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay: 0ms"></span>
                                    <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay: 140ms"></span>
                                    <span class="w-1.5 h-1.5 rounded-full bg-red-400 animate-bounce" style="animation-delay: 280ms"></span>
                                </div>
                            </div>
                        </div>
                    })}

                    <div id="chat-end"></div>
                </div>
            </div>

            <div class="border-t border-gray-100 dark:border-gray-900 px-6 py-4">
                <div class="max-w-3xl mx-auto flex gap-3 items-center">
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
    }
}
