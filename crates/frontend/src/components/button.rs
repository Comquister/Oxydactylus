use leptos::prelude::*;

#[component]
pub fn Button(
    #[prop(into, default = "primary".to_string())] variant: String,
    #[prop(default = false)] disabled: bool,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let base = "px-4 py-2 rounded font-medium transition-colors focus:outline-none";
    let style = move || match variant.as_str() {
        "secondary" => format!("{} bg-gray-200 hover:bg-gray-300 text-gray-800", base),
        "danger" => format!("{} bg-red-600 hover:bg-red-700 text-white", base),
        _ => format!("{} bg-blue-600 hover:bg-blue-700 text-white", base),
    };

    view! {
        <button
            class=style
            disabled=disabled
            on:click=move |_| {
                if !disabled {
                    if let Some(cb) = &on_click { cb.run(()); }
                }
            }
        >
            {children()}
        </button>
    }
}
