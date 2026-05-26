//! "The Technique" panel - displays raw technical internals.

use leptos::*;

/// The "Technique" panel - displays raw logs, API calls, and internals.
#[component]
pub fn TechniquePanel() -> impl IntoView {
    let logs = vec![
        "[2026-05-26 09:16] INFO: Initializing async runtime",
        "[2026-05-26 09:16] DEBUG: Compiling frontend to WASM",
        "[2026-05-26 09:16] TRACE: Memory allocation: 2.4MB",
        "[2026-05-26 09:17] INFO: HTTP Server listening on 0.0.0.0:3001",
        "[2026-05-26 09:17] DEBUG: Establishing DB connection pool",
        "[2026-05-26 09:17] INFO: ✓ All systems operational",
    ];

    view! {
        <div class="w-full h-full bg-black p-6 flex flex-col overflow-hidden">
            <div class="flex items-center space-x-2 mb-4 pb-2 border-b border-gray-700">
                <div class="w-3 h-3 bg-red-600 rounded-full"></div>
                <div class="w-3 h-3 bg-yellow-500 rounded-full"></div>
                <div class="w-3 h-3 bg-green-500 rounded-full"></div>
                <span class="text-gray-500 text-sm ml-4">pointe ~ backend</span>
            </div>

            <div class="flex-1 overflow-y-auto font-mono text-sm space-y-1">
                {logs.iter().map(|log| {
                    view! {
                        <div class="text-red-500">
                            {*log}
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            <div class="text-red-600 font-mono text-sm mt-4">
                {"pointe $ "}
                <span class="animate-pulse">_</span>
            </div>
        </div>
    }
}
