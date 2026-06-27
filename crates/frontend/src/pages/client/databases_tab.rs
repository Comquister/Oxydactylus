use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{ServerDatabase, CreateServerDatabaseRequest}};
use crate::components::ErrorBanner;
use crate::state::SessionContext;

#[component]
pub fn DatabasesTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let databases = RwSignal::new(Vec::<ServerDatabase>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let create_modal_open = RwSignal::new(false);
    let delete_modal_open = RwSignal::new(false);
    let host_id_input = RwSignal::new(String::new());
    let db_name_input = RwSignal::new(String::new());
    let db_username_input = RwSignal::new(String::new());
    let db_remote_input = RwSignal::new(String::new());
    let delete_db_id = RwSignal::new(String::new());

    let tok_load = session.token();
    let id_load = server_id.clone();
    let load_databases = Callback::new(move |_: ()| {
        let tok = tok_load.clone();
        let id = id_load.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match ApiClient::new(tok).get::<Vec<ServerDatabase>>(&format!("/servers/{}/databases", id)).await {
                Ok(dbs) => databases.set(dbs),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    spawn_local({
        let load = load_databases.clone();
        async move {
            load.run(());
        }
    });

    let on_create = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let host_id = host_id_input.get_untracked();
            let db_name = db_name_input.get_untracked();
            if host_id.is_empty() || db_name.is_empty() { return; }

            let body = CreateServerDatabaseRequest {
                host_id,
                database_name: db_name,
                username: {
                    let u = db_username_input.get_untracked();
                    if u.is_empty() { None } else { Some(u) }
                },
                remote: {
                    let r = db_remote_input.get_untracked();
                    if r.is_empty() { None } else { Some(r) }
                },
            };

            spawn_local(async move {
                match ApiClient::new(tok).post::<_, ServerDatabase>(&format!("/servers/{}/databases", id), &body).await {
                    Ok(_) => {
                        host_id_input.set(String::new());
                        db_name_input.set(String::new());
                        db_username_input.set(String::new());
                        db_remote_input.set(String::new());
                        create_modal_open.set(false);
                        load_databases.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_delete = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let db_id = delete_db_id.get_untracked();
            if db_id.is_empty() { return; }

            spawn_local(async move {
                match ApiClient::new(tok).delete(&format!("/servers/{}/databases/{}", id, db_id)).await {
                    Ok(_) => {
                        delete_modal_open.set(false);
                        delete_db_id.set(String::new());
                        load_databases.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    view! {
        <div class="space-y-4">
            <ErrorBanner msg=error />

            <button
                on:click=move |_| create_modal_open.set(true)
                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
            >
                "Create Database"
            </button>

            <Show when=move || !loading.get()>
                {move || {
                    let dbs = databases.get();
                    if dbs.is_empty() {
                        return view! {
                            <p class="text-gray-600">"No databases created yet."</p>
                        }.into_any();
                    }

                    view! {
                        <div class="rounded-lg border border-gray-200 overflow-hidden">
                            <table class="w-full text-sm text-left">
                                <thead class="bg-gray-50">
                                    <tr>
                                        <th class="px-4 py-3 font-semibold">Database Name</th>
                                        <th class="px-4 py-3 font-semibold">Username</th>
                                        <th class="px-4 py-3 font-semibold">Remote</th>
                                        <th class="px-4 py-3 font-semibold">Created</th>
                                        <th class="px-4 py-3 font-semibold">Actions</th>
                                    </tr>
                                </thead>
                                <tbody class="divide-y divide-gray-100">
                                    {dbs.into_iter().map(|db| {
                                        let db_id_delete = db.id.clone();
                                        let created_date = db.created_at.split('T').next().unwrap_or("—").to_string();
                                        view! {
                                            <tr class="hover:bg-gray-50">
                                                <td class="px-4 py-3 font-medium">{db.database_name.clone()}</td>
                                                <td class="px-4 py-3 text-gray-600">{db.username.clone()}</td>
                                                <td class="px-4 py-3 text-gray-600">{db.remote.clone()}</td>
                                                <td class="px-4 py-3 text-gray-600">
                                                    {created_date}
                                                </td>
                                                <td class="px-4 py-3">
                                                    <button
                                                        on:click=move |_| {
                                                            delete_db_id.set(db_id_delete.clone());
                                                            delete_modal_open.set(true);
                                                        }
                                                        class="text-red-600 hover:text-red-800 font-medium"
                                                    >
                                                        "Delete"
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </Show>

            // Create modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !create_modal_open.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| create_modal_open.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">Create Database</h3>
                        <button
                            on:click=move |_| create_modal_open.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium mb-1">Host ID</label>
                            <input
                                type="text"
                                placeholder="Database host ID"
                                value=move || host_id_input.get()
                                on:input=move |e| host_id_input.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Database Name</label>
                            <input
                                type="text"
                                placeholder="Database name"
                                value=move || db_name_input.get()
                                on:input=move |e| db_name_input.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Username (optional)</label>
                            <input
                                type="text"
                                placeholder="Database username"
                                value=move || db_username_input.get()
                                on:input=move |e| db_username_input.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Remote (optional)</label>
                            <input
                                type="text"
                                placeholder="e.g., % or localhost"
                                value=move || db_remote_input.get()
                                on:input=move |e| db_remote_input.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div class="flex gap-2 justify-end pt-4">
                            <button
                                on:click=move |_| create_modal_open.set(false)
                                class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=move |_| on_create.run(())
                                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700"
                            >
                                "Create"
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            // Delete confirm modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !delete_modal_open.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| delete_modal_open.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <h3 class="text-xl font-bold mb-4">Delete Database?</h3>
                    <p class="text-gray-600 mb-6">This action cannot be undone.</p>
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| delete_modal_open.set(false)
                            class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| on_delete.run(())
                            class="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700"
                        >
                            "Delete"
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
