use leptos::prelude::*;

const INPUT_CLASS: &str =
    "w-full px-3 py-2 border border-gray-300 rounded-md \
     focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white";

#[component]
pub fn TextInput(
    value: RwSignal<String>,
    #[prop(into, default = "".to_string())] placeholder: String,
    #[prop(into, default = "".to_string())] label: String,
    #[prop(default = "text")] input_type: &'static str,
) -> impl IntoView {
    view! {
        <div class="space-y-1">
            {(!label.is_empty()).then(|| view! {
                <label class="block text-sm font-medium text-gray-700">{label.clone()}</label>
            })}
            <input
                type=input_type
                placeholder=placeholder
                prop:value=value
                on:input=move |ev| value.set(event_target_value(&ev))
                class=INPUT_CLASS
            />
        </div>
    }
}

#[component]
pub fn SelectInput(
    value: RwSignal<String>,
    options: Vec<(String, String)>,
    #[prop(into, default = "".to_string())] label: String,
    #[prop(into, default = "Select...".to_string())] placeholder: String,
) -> impl IntoView {
    view! {
        <div class="space-y-1">
            {(!label.is_empty()).then(|| view! {
                <label class="block text-sm font-medium text-gray-700">{label.clone()}</label>
            })}
            <select
                prop:value=value
                on:change=move |ev| value.set(event_target_value(&ev))
                class=INPUT_CLASS
            >
                <option value="" disabled selected>{placeholder}</option>
                {options.into_iter().map(|(v, label)| view! {
                    <option value=v.clone()>{label}</option>
                }).collect_view()}
            </select>
        </div>
    }
}
