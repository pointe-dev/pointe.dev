use leptos::*;

#[component]
pub fn Hero(on_contact_click: impl Fn() + 'static) -> impl IntoView {
    view! {
        <section class="min-h-screen relative flex flex-col justify-center items-center text-center py-20 px-6 bg-gradient-to-b from-white to-gray-50 dark:from-black dark:to-gray-900 transition-colors duration-300 overflow-hidden">
            {/* Animated background gradient */}
            <div class="absolute inset-0 opacity-30 dark:opacity-20">
                <div class="absolute top-0 left-1/4 w-96 h-96 bg-red-200 dark:bg-red-900 rounded-full mix-blend-multiply filter blur-3xl animate-blob"></div>
                <div class="absolute top-0 right-1/4 w-96 h-96 bg-red-100 dark:bg-red-800 rounded-full mix-blend-multiply filter blur-3xl animate-blob animation-delay-2000"></div>
                <div class="absolute -bottom-8 left-1/2 w-96 h-96 bg-red-50 dark:bg-red-950 rounded-full mix-blend-multiply filter blur-3xl animate-blob animation-delay-4000"></div>
            </div>

            {/* Content container */}
            <div class="relative z-10">
                {/* Decorative accent line */}
                <div class="flex justify-center mb-8">
                    <div class="w-16 h-1 bg-gradient-to-r from-red-600 to-red-500 rounded-full"></div>
                </div>
                
                {/* Main headline */}
                <h1 class="text-5xl md:text-7xl font-sans font-bold mb-6 max-w-4xl leading-tight">
                    <span class="text-gray-900 dark:text-white">
                        "Sharp AI."
                    </span>
                    <span class="bg-gradient-to-r from-red-600 to-red-500 bg-clip-text text-transparent">
                        "Your operations, copiloted by your AI."
                    </span>
                </h1>
                
                {/* Subheading */}
                <p class="text-xl md:text-2xl text-gray-600 dark:text-gray-400 mb-12 max-w-3xl font-light">
                    "Pointe deploys autonomous agents that run your business processes — so you lead, not manage."
                </p>
                
                {/* Value propositions */}
                <div class="grid grid-cols-1 md:grid-cols-4 gap-6 w-full max-w-5xl mb-12">
                    <div class="p-4 text-center hover:scale-105 transition-transform duration-300">
                        <div class="text-3xl mb-2">"🚀"</div>
                        <h3 class="font-semibold text-gray-900 dark:text-white mb-1">"ready-to-use solutions"</h3>
                        <p class="text-sm text-gray-600 dark:text-gray-400">"pre-built automation templates"</p>
                    </div>
                    <div class="p-4 text-center hover:scale-105 transition-transform duration-300">
                        <div class="text-3xl mb-2">"💰"</div>
                        <h3 class="font-semibold text-gray-900 dark:text-white mb-1">"unbeatable pricing"</h3>
                        <p class="text-sm text-gray-600 dark:text-gray-400">"best ROI in the industry"</p>
                    </div>
                    <div class="p-4 text-center hover:scale-105 transition-transform duration-300">
                        <div class="text-3xl mb-2">"🧠"</div>
                        <h3 class="font-semibold text-gray-900 dark:text-white mb-1">"top models"</h3>
                        <p class="text-sm text-gray-600 dark:text-gray-400">"latest AI & LLM tech"</p>
                    </div>
                    <div class="p-4 text-center hover:scale-105 transition-transform duration-300">
                        <div class="text-3xl mb-2">"⚙"</div>
                        <h3 class="font-semibold text-gray-900 dark:text-white mb-1">"custom solutions"</h3>
                        <p class="text-sm text-gray-600 dark:text-gray-400">"tailored to your needs"</p>
                    </div>
                </div>
                
                {/* CTA Button */}
                <button
                    on:click=move |_| (on_contact_click)()
                    class="px-8 py-4 bg-gradient-to-r from-red-600 to-red-500 hover:from-red-700 hover:to-red-600 text-white rounded-lg font-semibold text-lg transition-all duration-300 hover:shadow-lg hover:shadow-red-600/50 transform hover:-translate-y-1"
                >
                    "start your automation journey"
                </button>
            </div>
        </section>
    }
}
