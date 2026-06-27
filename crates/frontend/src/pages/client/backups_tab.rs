use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{Backup, CreateBackupRequest}};
use crate::components::{ErrorBanner};
use crate::state::SessionContext;

#[component]
pub fn BackupsTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let backups = RwSignal::new(Vec::<Backup>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let create_modal_open = RwSignal::new(false);
    let delete_modal_open = RwSignal::new(false);
    let backup_name = RwSignal::new(String::new());
    let ignored_files = RwSignal::new(String::new());
    let delete_backup_id = RwSignal::new(String::new());

    let tok_load = session.token();
    let id_load = server_id.clone();
    let load_backups = Callback::new(move |_: ()| {
        let tok = tok_load.clone();
        let id = id_load.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match ApiClient::new(tok).get::<Vec<Backup>>(&format!("/servers/{}/backups", id)).await {
                Ok(bkps) => backups.set(bkps),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    spawn_local({
        let load = load_backups.clone();
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
            let name = backup_name.get_untracked();
            if name.is_empty() { return; }

            let ignored = ignored_files.get_untracked();
            let ignored_list: Option<Vec<String>> = if ignored.is_empty() {
                None
            } else {
                Some(ignored.split('\n').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            };

            let body = CreateBackupRequest {
                name,
                ignored_files: ignored_list,
            };

            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Backup>(&format!("/servers/{}/backups", id), &body).await {
                    Ok(_) => {
                        backup_name.set(String::new());
                        ignored_files.set(String::new());
                        create_modal_open.set(false);
                        load_backups.run(());
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
            let backup_id = delete_backup_id.get_untracked();
            if backup_id.is_empty() { return; }

            spawn_local(async move {
                match ApiClient::new(tok).delete(&format!("/servers/{}/backups/{}", id, backup_id)).await {
                    Ok(_) => {
                        delete_modal_open.set(false);
                        delete_backup_id.set(String::new());
                        load_backups.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_toggle_lock = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |backup_id: String| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).post::<(), serde_json::Value>(&format!("/servers/{}/backups/{}/lock", id, backup_id), &()).await {
                    Ok(_) => {
                        load_backups.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let format_bytes = |bytes: i64| {
        if bytes < 1024 {
            format!("{}B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    };

    view! {
        <div class="space-y-4">
            <ErrorBanner msg=error />

            <button
                on:click=move |_| create_modal_open.set(true)
                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
            >
                "Create Backup"
            </button>

            <Show when=move || !loading.get()>
                {move || {
                    let bkps = backups.get();
                    if bkps.is_empty() {
                        return view! {
                            <p class="text-gray-600">"No backups created yet."</p>
                        }.into_any();
                    }

                    view! {
                        <div class="rounded-lg border border-gray-200 overflow-hidden">
                            <table class="w-full text-sm text-left">
                                <thead class="bg-gray-50">
                                    <tr>
                                        <th class="px-4 py-3 font-semibold">Name</th>
                                        <th class="px-4 py-3 font-semibold">Size</th>
                                        <th class="px-4 py-3 font-semibold">Status</th>
                                        <th class="px-4 py-3 font-semibold">Locked</th>
                                        <th class="px-4 py-3 font-semibold">Created</th>
                                        <th class="px-4 py-3 font-semibold">Actions</th>
                                    </tr>
                                </thead>
                                <tbody class="divide-y divide-gray-100">
                                    {bkps.into_iter().map(|backup| {
                                        let backup_id_delete = backup.id.clone();
                                        let backup_id_lock = backup.id.clone();
                                        let created_date = backup.created_at.split('T').next().unwrap_or("—").to_string();
                                        view! {
                                            <tr class="hover:bg-gray-50">
                                                <td class="px-4 py-3 font-medium">{backup.name.clone()}</td>
                                                <td class="px-4 py-3 text-gray-600">{format_bytes(backup.bytes)}</td>
                                                <td class="px-4 py-3">
                                                    {if backup.is_successful {
                                                        view! { <span class="px-2 py-1 bg-green-100 text-green-800 rounded text-xs font-medium">"Successful"</span> }
                                                    } else {
                                                        view! { <span class="px-2 py-1 bg-red-100 text-red-800 rounded text-xs font-medium">"Failed"</span> }
                                                    }}
                                                </td>
                                                <td class="px-4 py-3">
                                                    {if backup.is_locked {
                                                        view! { <span class="px-2 py-1 bg-yellow-100 text-yellow-800 rounded text-xs font-medium">"🔒 Locked"</span> }
                                                    } else {
                                                        view! { <span class="px-2 py-1 bg-gray-100 text-gray-800 rounded text-xs font-medium">"Unlocked"</span> }
                                                    }}
                                                </td>
                                                <td class="px-4 py-3 text-gray-600">
                                                    {created_date}
                                                </td>
                                                <td class="px-4 py-3 space-x-2">
                                                    <button
                                                        on:click=move |_| on_toggle_lock.run(backup_id_lock.clone())
                                                        class="text-blue-600 hover:text-blue-800 font-medium text-sm"
                                                    >
                                                        {if backup.is_locked { "Unlock" } else { "Lock" }}
                                                    </button>
                                                    <button
                                                        on:click=move |_| {
                                                            delete_backup_id.set(backup_id_delete.clone());
                                                            delete_modal_open.set(true);
                                                        }
                                                        class="text-red-600 hover:text-red-800 font-medium text-sm"
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

            // Create backup modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !create_modal_open.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| create_modal_open.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">Create Backup</h3>
                        <button
                            on:click=move |_| create_modal_open.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium mb-1">Backup Name</label>
                            <input
                                type="text"
                                placeholder="e.g., Backup before update"
                                value=move || backup_name.get()
                                on:input=move |e| backup_name.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Ignored Files (optional)</label>
                            <textarea
                                placeholder="One file per line (e.g., /path/to/file)"
                                prop:value=move || ignored_files.get()
                                on:input=move |e| ignored_files.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                                rows="4"
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
                    <h3 class="text-xl font-bold mb-4">Delete Backup?</h3>
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
