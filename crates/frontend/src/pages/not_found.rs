use leptos::prelude::*;

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div class="flex items-center justify-center h-screen">
            <div class="text-center">
                <h1 class="text-6xl font-bold text-gray-300">"404"</h1>
                <p class="text-gray-500 mt-2">"Page not found"</p>
            </div>
        </div>
    }
}
