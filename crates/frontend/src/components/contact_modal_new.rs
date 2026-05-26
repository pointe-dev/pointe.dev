use leptos::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContactStep {
    Start,
    AiChat,
    ContactInfo,
    Calendly,
}

#[component]
pub fn ContactModal(is_open: RwSignal<bool>) -> impl IntoView {
    let (current_step, set_current_step) = create_signal(ContactStep::Start);
    let (ai_response, set_ai_response) = create_signal(String::new());
    let (use_case_input, set_use_case_input) = create_signal(String::new());
    let (name, set_name) = create_signal(String::new());
    let (email, set_email) = create_signal(String::new());
    let (_message, _set_message) = create_signal(String::new());
    let (contact_pref, set_contact_pref) = create_signal("email".to_string());
    let (loading, set_loading) = create_signal(false);
    
    let handle_step_1_start = move |_| {
        set_current_step.set(ContactStep::AiChat);
    };
    
    let handle_ai_chat_submit = move |_| {
        let use_case = use_case_input.get();
        if use_case.trim().is_empty() {
            return;
        }
        
        set_loading.set(true);
        
        // Simulate AI response
        let response = format!(
            "Thanks for sharing: \"{}\"\n\nBased on your use case, we can help you:\n• Automate repetitive workflows\n• Reduce manual hours by up to 80%\n• Set up monitoring and alerts\n\nReady to move forward?",
            use_case
        );
        
        set_ai_response.set(response);
        set_loading.set(false);
    };
    
    let proceed_to_contact = move |_| {
        set_current_step.set(ContactStep::ContactInfo);
    };
    
    let handle_contact_info_submit = move |_| {
        let name_val = name.get();
        let email_val = email.get();
        
        if name_val.trim().is_empty() || email_val.trim().is_empty() {
            return;
        }
        
        set_current_step.set(ContactStep::Calendly);
    };
    
    let skip_to_calendly = move |_| {
        set_current_step.set(ContactStep::Calendly);
    };
    
    let close_modal = move |_| {
        is_open.set(false);
        set_current_step.set(ContactStep::Start);
    };
    
    view! {
        {move || if is_open.get() {
            view! {
                <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
                    <div class="bg-white dark:bg-black border border-gray-200 dark:border-gray-800 rounded-lg max-w-2xl w-full max-h-[90vh] overflow-y-auto">
                        {/* Header */}
                        <div class="flex justify-between items-center p-6 border-b border-gray-200 dark:border-gray-800">
                            <h2 class="text-2xl font-bold">
                                {move || match current_step.get() {
                                    ContactStep::Start => "Let's Talk",
                                    ContactStep::AiChat => "Describe Your Use Case",
                                    ContactStep::ContactInfo => "Your Information",
                                    ContactStep::Calendly => "Schedule a Call",
                                }}
                            </h2>
                            <button
                                on:click=close_modal
                                class="text-2xl hover:text-red-600 transition-colors"
                            >
                                "X"
                            </button>
                        </div>
                        
                        {/* Content */}
                        <div class="p-6">
                            {move || match current_step.get() {
                                ContactStep::Start => view! {
                                    <div class="space-y-4">
                                        <p class="text-gray-600 dark:text-gray-400">
                                            "Choose how you'd like to get started:"
                                        </p>
                                        <button
                                            on:click=handle_step_1_start
                                            class="w-full p-4 border border-gray-300 dark:border-gray-700 rounded-lg hover:border-red-600 dark:hover:border-red-600 hover:bg-gray-50 dark:hover:bg-gray-900 transition-colors"
                                        >
                                            <div class="text-lg font-semibold">"Talk to Our AI"</div>
                                            <div class="text-sm text-gray-600 dark:text-gray-400">
                                                "Describe your use case and get instant insights"
                                            </div>
                                        </button>
                                        <button
                                            on:click=skip_to_calendly
                                            class="w-full p-4 border border-gray-300 dark:border-gray-700 rounded-lg hover:border-red-600 dark:hover:border-red-600 hover:bg-gray-50 dark:hover:bg-gray-900 transition-colors"
                                        >
                                            <div class="text-lg font-semibold">"Schedule a Call"</div>
                                            <div class="text-sm text-gray-600 dark:text-gray-400">
                                                "Book a direct meeting with our team"
                                            </div>
                                        </button>
                                    </div>
                                }.into_view(),
                                
                                ContactStep::AiChat => view! {
                                    <div class="space-y-4">
                                        <textarea
                                            placeholder="Tell us about your business challenge..."
                                            on:input=move |ev| set_use_case_input.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-black dark:text-white focus:outline-none focus:border-red-600"
                                            rows=4
                                        />
                                        {move || if !ai_response.get().is_empty() {
                                            view! {
                                                <div class="p-4 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-800">
                                                    <p class="text-sm text-gray-600 dark:text-gray-400 whitespace-pre-line">
                                                        {ai_response.get()}
                                                    </p>
                                                </div>
                                            }.into_view()
                                        } else {
                                            "".into_view()
                                        }}
                                        <div class="flex gap-2 pt-4">
                                            <button
                                                on:click=handle_ai_chat_submit
                                                disabled=loading.get()
                                                class="flex-1 px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50 transition-colors"
                                            >
                                                {move || if loading.get() { "Thinking..." } else { "Get Insights" }}
                                            </button>
                                            {move || if !ai_response.get().is_empty() {
                                                view! {
                                                    <button
                                                        on:click=proceed_to_contact
                                                        class="flex-1 px-4 py-2 border border-red-600 text-red-600 rounded-lg hover:bg-red-50 dark:hover:bg-gray-900 transition-colors"
                                                    >
                                                        "Next Step"
                                                    </button>
                                                }.into_view()
                                            } else {
                                                "".into_view()
                                            }}
                                        </div>
                                    </div>
                                }.into_view(),
                                
                                ContactStep::ContactInfo => view! {
                                    <div class="space-y-4">
                                        <input
                                            type="text"
                                            placeholder="Your name"
                                            on:input=move |ev| set_name.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-black dark:text-white focus:outline-none focus:border-red-600"
                                        />
                                        <input
                                            type="email"
                                            placeholder="your@email.com"
                                            on:input=move |ev| set_email.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-black dark:text-white focus:outline-none focus:border-red-600"
                                        />
                                        <textarea
                                            placeholder="Tell us more about your project..."
                                            on:input=move |ev| _set_message.set(event_target_value(&ev))
                                            class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-black dark:text-white focus:outline-none focus:border-red-600"
                                            rows=3
                                        />
                                        <div class="space-y-2">
                                            <label class="block text-sm font-medium">"Preferred contact method:"</label>
                                            <div class="flex gap-4">
                                                <label class="flex items-center gap-2">
                                                    <input
                                                        type="radio"
                                                        name="contact-pref"
                                                        value="email"
                                                        on:change=move |ev| set_contact_pref.set(event_target_value(&ev))
                                                        checked=contact_pref.get() == "email"
                                                    />
                                                    "Email"
                                                </label>
                                                <label class="flex items-center gap-2">
                                                    <input
                                                        type="radio"
                                                        name="contact-pref"
                                                        value="call"
                                                        on:change=move |ev| set_contact_pref.set(event_target_value(&ev))
                                                        checked=contact_pref.get() == "call"
                                                    />
                                                    "Phone Call"
                                                </label>
                                            </div>
                                        </div>
                                        <div class="flex gap-2 pt-4">
                                            <button
                                                on:click=move |_| set_current_step.set(ContactStep::AiChat)
                                                class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 text-black dark:text-white rounded-lg hover:bg-gray-50 dark:hover:bg-gray-900 transition-colors"
                                            >
                                                "Back"
                                            </button>
                                            <button
                                                on:click=handle_contact_info_submit
                                                class="flex-1 px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
                                            >
                                                "Continue"
                                            </button>
                                        </div>
                                    </div>
                                }.into_view(),
                                
                                ContactStep::Calendly => view! {
                                    <div class="space-y-4">
                                        <p class="text-gray-600 dark:text-gray-400">
                                            "Great! Let's schedule a time to discuss your project."
                                        </p>
                                        <div class="p-4 bg-gray-50 dark:bg-gray-900 rounded-lg text-center">
                                            <a
                                                href="https://calendly.com/example"
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                class="inline-block px-6 py-3 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
                                            >
                                                "Open Calendar"
                                            </a>
                                            <p class="text-sm text-gray-600 dark:text-gray-400 mt-4">
                                                "(Opens in new window)"
                                            </p>
                                        </div>
                                        <button
                                            on:click=close_modal
                                            class="w-full px-4 py-2 bg-gray-200 dark:bg-gray-800 text-black dark:text-white rounded-lg hover:bg-gray-300 dark:hover:bg-gray-700 transition-colors"
                                        >
                                            "Close"
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
