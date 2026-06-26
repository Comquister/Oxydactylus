use leptos::prelude::*;

pub struct Column<T> {
    pub header: &'static str,
    pub render: fn(&T) -> String,
}

#[component]
pub fn Table<T: Clone + 'static>(
    columns: Vec<Column<T>>,
    rows: Vec<T>,
    #[prop(optional)] on_delete: Option<Callback<String>>,
    key_fn: fn(&T) -> String,
) -> impl IntoView {
    let has_actions = on_delete.is_some();

    view! {
        <div class="overflow-x-auto rounded-lg border border-gray-200">
            <table class="w-full text-sm text-left">
                <thead class="bg-gray-50 text-xs text-gray-700 uppercase">
                    <tr>
                        {columns.iter().map(|c| view! {
                            <th class="px-6 py-3 font-semibold">{c.header}</th>
                        }).collect_view()}
                        {has_actions.then(|| view! {
                            <th class="px-6 py-3 font-semibold">"Actions"</th>
                        })}
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-100">
                    {rows.into_iter().map(|row| {
                        let row_key = key_fn(&row);
                        view! {
                            <tr class="hover:bg-gray-50">
                                {columns.iter().map(|c| view! {
                                    <td class="px-6 py-3 text-gray-700">{(c.render)(&row)}</td>
                                }).collect_view()}
                                {on_delete.map(|cb| {
                                    let id = row_key.clone();
                                    view! {
                                        <td class="px-6 py-3">
                                            <button
                                                on:click=move |_| cb.run(id.clone())
                                                class="text-red-600 hover:text-red-800 font-medium"
                                            >
                                                "Delete"
                                            </button>
                                        </td>
                                    }
                                })}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
