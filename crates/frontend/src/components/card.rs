use leptos::prelude::*;

#[component]
pub fn Card(
    #[prop(into)] title: String,
    #[prop(into, default = "".to_string())] subtitle: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="bg-white rounded-lg shadow p-6">
            <div class="mb-4">
                <h2 class="text-xl font-bold text-gray-900">{title}</h2>
                {(!subtitle.is_empty()).then(|| view! {
                    <p class="text-sm text-gray-500 mt-1">{subtitle}</p>
                })}
            </div>
            {children()}
        </div>
    }
}
