use leptos::*;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="max-w-7xl mx-auto px-6 py-20 text-center">
            <h1 class="text-4xl font-bold text-red-600 mb-4">404</h1>
            <p class="text-gray-400">Page not found.</p>
        </div>
    }
}
