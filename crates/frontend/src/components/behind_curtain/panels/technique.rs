use leptos::*;

#[component]
pub fn TechniquePanel() -> impl IntoView {
    let logs: Vec<(&str, &str, &str)> = vec![
        ("[09:16:03]", "BOOT ", " tokio runtime — 4 async workers"),
        ("[09:16:04]", "WASM ", " frontend compiled → 1.2MB → gzip 374KB"),
        ("[09:16:04]", "HTTP ", " axum listening 0.0.0.0:3001"),
        ("[09:16:09]", "SCHED", " automation #12 — lead:FR-0847 triggered"),
        ("[09:16:09]", "EXEC ", " crm.score_lead(id=FR-0847) → 94/100"),
        ("[09:16:10]", "EXEC ", " crm.assign(rep=alice) → ok"),
        ("[09:16:10]", "NOTIF", " slack #sales — sent (11ms)"),
        ("[09:16:14]", "REQ  ", " POST /api/ai/chat 200 — 38ms"),
    ];

    view! {
        <div class="w-full h-full bg-black px-6 py-5 flex flex-col overflow-hidden">
            // Terminal header
            <div class="flex items-center gap-2 mb-4 pb-3 border-b border-gray-800 shrink-0">
                <div class="w-2.5 h-2.5 bg-red-600 rounded-full"></div>
                <div class="w-2.5 h-2.5 bg-yellow-500/60 rounded-full"></div>
                <div class="w-2.5 h-2.5 bg-green-500/60 rounded-full"></div>
                <span class="text-gray-600 text-xs ml-3 font-mono">"pointe ~ backend"</span>
            </div>

            // Log lines
            <div class="flex-1 overflow-hidden font-mono text-xs space-y-1.5">
                {logs.iter().map(|(time, level, msg)| {
                    let level_color = match level.trim() {
                        "BOOT" | "HTTP" => "text-gray-500",
                        "SCHED" => "text-red-500",
                        "EXEC"  => "text-red-400",
                        "NOTIF" => "text-red-300",
                        "WASM"  => "text-gray-600",
                        _       => "text-gray-600",
                    };
                    view! {
                        <div class="flex gap-2">
                            <span class="text-gray-700 shrink-0">{*time}</span>
                            <span class=level_color>{*level}</span>
                            <span class="text-gray-500">{*msg}</span>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Prompt
            <div class="mt-3 font-mono text-xs text-red-600 shrink-0">
                {"pointe $ "}
                <span class="animate-pulse">"_"</span>
            </div>
        </div>
    }
}
