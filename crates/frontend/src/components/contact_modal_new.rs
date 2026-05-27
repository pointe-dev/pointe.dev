use leptos::*;
use crate::i18n::{Lang, t};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContactStep {
    Start,
    AiChat,
    ContactInfo,
    Calendly,
}

#[component]
pub fn ContactModal(
    is_open: RwSignal<bool>,
    on_chat: impl Fn() + 'static,
) -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    let (current_step, set_current_step) = create_signal(ContactStep::Start);
    let (ai_response, set_ai_response) = create_signal(String::new());
    let (use_case_input, set_use_case_input) = create_signal(String::new());
    let (name, set_name) = create_signal(String::new());
    let (email, set_email) = create_signal(String::new());
    let (_message, _set_message) = create_signal(String::new());
    let (contact_pref, set_contact_pref) = create_signal("email".to_string());
    let (loading, set_loading) = create_signal(false);

    let handle_ai_chat_submit = move |_| {
        let use_case = use_case_input.get();
        if use_case.trim().is_empty() { return; }
        set_loading.set(true);
        let response = format!(
            "{}\n\n• {}\n• {}\n• {}",
            use_case,
            "Automatiser les workflows répétitifs",
            "Réduire les heures manuelles jusqu'à 80 %",
            "Mettre en place supervision et alertes"
        );
        set_ai_response.set(response);
        set_loading.set(false);
    };

    let handle_contact_info_submit = move |_| {
        if name.get().trim().is_empty() || email.get().trim().is_empty() { return; }
        set_current_step.set(ContactStep::Calendly);
    };

    let on_chat = store_value(on_chat);

    let close_modal = move |_| {
        is_open.set(false);
        set_current_step.set(ContactStep::Start);
    };

    let on_chat_click = move |_| {
        is_open.set(false);
        set_current_step.set(ContactStep::Start);
        on_chat.with_value(|f| f());
    };

    let step_title = move || match current_step.get() {
        ContactStep::Start => t(lang.get(), "modal.title"),
        ContactStep::AiChat => t(lang.get(), "modal.ai_step"),
        ContactStep::ContactInfo => t(lang.get(), "modal.contact_title"),
        ContactStep::Calendly => t(lang.get(), "modal.cal_title"),
    };

    view! {
        {move || if is_open.get() {
            view! {
                <div
                    class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4 animate-backdrop-in"
                    on:click=move |ev| {
                        if ev.target() == ev.current_target() { is_open.set(false); }
                    }
                >
                    <div class="bg-white dark:bg-black border border-gray-200 dark:border-gray-800 rounded-xl max-w-lg w-full max-h-[90vh] overflow-y-auto shadow-2xl animate-modal-enter">

                        {/* Header */}
                        <div class="flex justify-between items-center px-6 py-5 border-b border-gray-100 dark:border-gray-900">
                            <div class="flex items-center gap-3">
                                {move || (current_step.get() != ContactStep::Start).then(|| view! {
                                    <button
                                        on:click=move |_| {
                                            set_current_step.set(match current_step.get() {
                                                ContactStep::AiChat | ContactStep::Calendly => ContactStep::Start,
                                                ContactStep::ContactInfo => ContactStep::AiChat,
                                                ContactStep::Start => ContactStep::Start,
                                            });
                                        }
                                        class="text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors mr-1 text-sm"
                                    >
                                        "←"
                                    </button>
                                })}
                                <h2 class="text-xl font-bold text-gray-900 dark:text-white">
                                    {move || step_title()}
                                </h2>
                            </div>
                            <button
                                on:click=close_modal
                                class="w-8 h-8 flex items-center justify-center rounded-full text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-gray-900 transition-colors"
                            >
                                "✕"
                            </button>
                        </div>

                        {/* Content with step transition */}
                        <div class="px-6 py-6">
                            {move || match current_step.get() {
                                ContactStep::Start => view! {
                                    <div class="space-y-3 animate-step-enter">
                                        <p class="text-sm text-gray-500 dark:text-gray-400 mb-5">
                                            {move || t(lang.get(), "modal.choose")}
                                        </p>
                                        {/* Talk to AI → navigates to Chat page */}
                                        <button
                                            on:click=on_chat_click
                                            class="w-full p-4 border border-gray-200 dark:border-gray-700 rounded-xl hover:border-red-600 hover:bg-red-50 dark:hover:bg-red-950/20 transition-all text-left group"
                                        >
                                            <div class="flex items-start gap-3">
                                                <span class="text-xl mt-0.5">"💬"</span>
                                                <div>
                                                    <p class="font-semibold text-gray-900 dark:text-white group-hover:text-red-600 transition-colors">
                                                        {move || t(lang.get(), "modal.ai_cta")}
                                                    </p>
                                                    <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5">
                                                        {move || t(lang.get(), "modal.ai_desc")}
                                                    </p>
                                                </div>
                                            </div>
                                        </button>
                                        {/* Schedule a Call */}
                                        <button
                                            on:click=move |_| set_current_step.set(ContactStep::Calendly)
                                            class="w-full p-4 border border-gray-200 dark:border-gray-700 rounded-xl hover:border-red-600 hover:bg-red-50 dark:hover:bg-red-950/20 transition-all text-left group"
                                        >
                                            <div class="flex items-start gap-3">
                                                <span class="text-xl mt-0.5">"📅"</span>
                                                <div>
                                                    <p class="font-semibold text-gray-900 dark:text-white group-hover:text-red-600 transition-colors">
                                                        {move || t(lang.get(), "modal.cal_cta")}
                                                    </p>
                                                    <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5">
                                                        {move || t(lang.get(), "modal.cal_desc")}
                                                    </p>
                                                </div>
                                            </div>
                                        </button>
                                    </div>
                                }.into_view(),

                                ContactStep::AiChat => view! {
                                    <div class="space-y-4 animate-step-enter">
                                        <textarea
                                            placeholder=move || t(lang.get(), "modal.ai_ph")
                                            on:input=move |ev| set_use_case_input.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-200 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:outline-none focus:border-red-600 transition-colors text-sm leading-relaxed resize-none"
                                            rows=4
                                        />
                                        {move || (!ai_response.get().is_empty()).then(|| view! {
                                            <div class="p-4 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-100 dark:border-gray-800 animate-step-enter">
                                                <p class="text-sm text-gray-600 dark:text-gray-400 whitespace-pre-line leading-relaxed">
                                                    {ai_response.get()}
                                                </p>
                                            </div>
                                        })}
                                        <div class="flex gap-2">
                                            <button
                                                on:click=handle_ai_chat_submit
                                                disabled=move || loading.get()
                                                class="flex-1 px-4 py-2.5 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50 transition-colors text-sm font-semibold"
                                            >
                                                {move || if loading.get() { t(lang.get(), "modal.thinking") } else { t(lang.get(), "modal.insights") }}
                                            </button>
                                            {move || (!ai_response.get().is_empty()).then(|| view! {
                                                <button
                                                    on:click=move |_| set_current_step.set(ContactStep::ContactInfo)
                                                    class="flex-1 px-4 py-2.5 border border-red-600 text-red-600 rounded-lg hover:bg-red-50 dark:hover:bg-gray-900 transition-colors text-sm font-semibold"
                                                >
                                                    {move || t(lang.get(), "modal.next")}
                                                </button>
                                            })}
                                        </div>
                                    </div>
                                }.into_view(),

                                ContactStep::ContactInfo => view! {
                                    <div class="space-y-3 animate-step-enter">
                                        <input
                                            type="text"
                                            placeholder=move || t(lang.get(), "modal.name_ph")
                                            on:input=move |ev| set_name.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-200 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:outline-none focus:border-red-600 transition-colors text-sm"
                                        />
                                        <input
                                            type="email"
                                            placeholder=move || t(lang.get(), "modal.email_ph")
                                            on:input=move |ev| set_email.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-200 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:outline-none focus:border-red-600 transition-colors text-sm"
                                        />
                                        <textarea
                                            placeholder=move || t(lang.get(), "modal.msg_ph")
                                            on:input=move |ev| _set_message.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-200 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:outline-none focus:border-red-600 transition-colors text-sm leading-relaxed resize-none"
                                            rows=3
                                        />
                                        <div class="pt-1">
                                            <p class="text-xs text-gray-500 mb-2">{move || t(lang.get(), "modal.pref")}</p>
                                            <div class="flex gap-4 text-sm">
                                                <label class="flex items-center gap-2 cursor-pointer">
                                                    <input type="radio" name="contact-pref" value="email"
                                                        on:change=move |ev| set_contact_pref.set(event_target_value(&ev))
                                                        checked=move || contact_pref.get() == "email"
                                                    />
                                                    {move || t(lang.get(), "modal.email_pref")}
                                                </label>
                                                <label class="flex items-center gap-2 cursor-pointer">
                                                    <input type="radio" name="contact-pref" value="call"
                                                        on:change=move |ev| set_contact_pref.set(event_target_value(&ev))
                                                        checked=move || contact_pref.get() == "call"
                                                    />
                                                    {move || t(lang.get(), "modal.call_pref")}
                                                </label>
                                            </div>
                                        </div>
                                        <button
                                            on:click=handle_contact_info_submit
                                            class="w-full px-4 py-2.5 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors text-sm font-semibold mt-2"
                                        >
                                            {move || t(lang.get(), "modal.continue")}
                                        </button>
                                    </div>
                                }.into_view(),

                                ContactStep::Calendly => view! {
                                    <div class="space-y-4 animate-step-enter">
                                        <p class="text-sm text-gray-500 dark:text-gray-400">
                                            {move || t(lang.get(), "modal.cal_body")}
                                        </p>
                                        <div class="p-6 bg-gray-50 dark:bg-gray-900 rounded-xl text-center border border-gray-100 dark:border-gray-800">
                                            <a
                                                href="https://cal.com"
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                class="inline-block px-6 py-3 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors font-semibold text-sm"
                                            >
                                                {move || t(lang.get(), "modal.open_cal")}
                                            </a>
                                            <p class="text-xs text-gray-400 mt-3">
                                                {move || t(lang.get(), "modal.tab_note")}
                                            </p>
                                        </div>
                                        <button
                                            on:click=close_modal
                                            class="w-full px-4 py-2.5 border border-gray-200 dark:border-gray-700 text-gray-600 dark:text-gray-400 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-900 transition-colors text-sm"
                                        >
                                            {move || t(lang.get(), "modal.close")}
                                        </button>
                                    </div>
                                }.into_view(),
                            }}
                        </div>
                    </div>
                </div>
            }.into_view()
        } else {
            "".into_view()
        }}
    }
}
