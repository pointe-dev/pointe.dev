use leptos::*;

#[component]
pub fn Hero(on_contact_click: impl Fn() + 'static) -> impl IntoView {
    view! {
        <section class="min-h-screen relative flex flex-col justify-center items-center text-center py-20 px-6 overflow-hidden bg-white dark:bg-black">

            {/* Background — assertive, alive */}
            <div class="absolute inset-0 pointer-events-none select-none">
                <div class="absolute -top-32 -left-32 w-[700px] h-[700px] bg-red-600/10 dark:bg-red-600/15 rounded-full blur-3xl animate-blob"></div>
                <div class="absolute bottom-0 right-0 w-[500px] h-[500px] bg-red-500/8 dark:bg-red-500/12 rounded-full blur-3xl animate-blob animation-delay-2000"></div>
                <div class="absolute top-1/2 left-1/2 w-[300px] h-[300px] bg-red-400/5 dark:bg-red-400/8 rounded-full blur-3xl animate-blob animation-delay-4000"></div>
            </div>

            {/* Content */}
            <div class="relative z-10 max-w-5xl w-full">

                {/* Eyebrow badge */}
                <div class="flex justify-center mb-10 animate-fade-up">
                    <span class="inline-flex items-center gap-2 px-4 py-2 border border-red-600/30 text-red-600 text-xs font-semibold tracking-widest uppercase rounded-full">
                        <span class="w-2 h-2 rounded-full bg-red-600 animate-pulse inline-block"></span>
                        "AI Automation Agency"
                    </span>
                </div>

                {/* Headline */}
                <h1 class="font-bold tracking-tight leading-none mb-4 animate-fade-up stagger-1">
                    <span class="text-5xl md:text-7xl text-gray-900 dark:text-white block">
                        "Sharp automation."
                    </span>
                    <span
                        class="text-5xl md:text-7xl text-red-600 relative block overflow-hidden"
                        style="height: 1.15em; margin-top: 0.05em;"
                    >
                        <span class="word-slot word-slot-1">"For founders."</span>
                        <span class="word-slot word-slot-2">"For sales leaders."</span>
                        <span class="word-slot word-slot-3">"For ops teams."</span>
                        <span class="word-slot word-slot-4">"For growth teams."</span>
                        <span class="word-slot word-slot-5">"For builders."</span>
                    </span>
                </h1>

                {/* Sub-copy */}
                <p class="text-xl md:text-2xl text-gray-500 dark:text-gray-400 mb-12 mt-8 max-w-2xl mx-auto font-light leading-relaxed animate-fade-up stagger-2">
                    "Autonomous agents that run your business processes — so you lead, not manage."
                </p>

                {/* CTAs */}
                <div class="flex flex-col gap-4 justify-center items-center animate-fade-up stagger-3">
                    <button
                        on:click=move |_| (on_contact_click)()
                        class="px-10 py-4 bg-red-600 text-white rounded-lg font-semibold text-lg hover:bg-red-700 transition-all duration-200 hover:shadow-red-glow hover:-translate-y-0.5 w-full max-w-xs"
                    >
                        "Start your automation →"
                    </button>
                    <a
                        href="#gallery"
                        class="px-10 py-4 border border-gray-200 dark:border-gray-800 text-gray-700 dark:text-gray-300 rounded-lg font-semibold text-lg hover:border-red-600 hover:text-red-600 transition-all duration-200 hover:-translate-y-0.5 w-full max-w-xs"
                    >
                        "See solutions"
                    </a>
                </div>
            </div>

            {/* Scroll indicator */}
            <div class="absolute bottom-8 left-1/2 -translate-x-1/2 flex flex-col items-center gap-2 animate-fade-up stagger-4">
                <span class="text-xs text-gray-400 dark:text-gray-600 tracking-widest uppercase font-medium">"Scroll"</span>
                <div class="w-px h-8 bg-gradient-to-b from-gray-300 to-transparent dark:from-gray-700 animate-scroll-bounce"></div>
            </div>
        </section>
    }
}
