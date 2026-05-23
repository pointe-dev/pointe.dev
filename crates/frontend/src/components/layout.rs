use leptos::*;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen bg-black text-white">
            <nav class="bg-black border-b border-gray-900">
                <div class="max-w-7xl mx-auto px-6 py-4">
                    <h1 class="text-2xl font-bold tracking-tight">
                        <span class="text-red-600">pointe</span><span class="text-white">.dev</span>
                    </h1>
                </div>
            </nav>
            <main>
                {children()}
            </main>
        </div>
    }
}
