use leptos::prelude::*;

#[component]
pub fn ErrorBanner(msg: RwSignal<String>) -> impl IntoView {
    view! {
        <Show when=move || !msg.get().is_empty()>
            <div class="p-3 bg-red-100 border border-red-400 text-red-700 rounded-md text-sm">
                {move || msg.get()}
            </div>
        </Show>
    }
}
