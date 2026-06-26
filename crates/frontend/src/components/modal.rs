use leptos::prelude::*;

#[component]
pub fn Modal(
    #[prop(into)] title: String,
    open: RwSignal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Show when=move || open.get()>
            <div class="fixed inset-0 z-50 flex items-center justify-center">
                <div
                    class="absolute inset-0 bg-black bg-opacity-50"
                    on:click=move |_| open.set(false)
                />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">{title.clone()}</h3>
                        <button
                            on:click=move |_| open.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    {children()}
                </div>
            </div>
        </Show>
    }
}
