use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{UpdateServerRequest, ChangeEggRequest}};
use crate::components::{ErrorBanner};
use crate::state::SessionContext;

#[component]
pub fn SettingsTab(server_id: String, is_admin: bool, is_owner: bool) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let error = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    let rename_modal_open = RwSignal::new(false);
    let new_server_name = RwSignal::new(String::new());

    let egg_modal_open = RwSignal::new(false);
    let new_egg_id = RwSignal::new(String::new());

    let suspend_confirm = RwSignal::new(false);
    let unsuspend_confirm = RwSignal::new(false);
    let reinstall_confirm = RwSignal::new(false);

    let on_rename = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let name = new_server_name.get_untracked();
            if name.is_empty() { return; }

            let body = UpdateServerRequest { name: Some(name) };
            spawn_local(async move {
                match ApiClient::new(tok).patch::<_, serde_json::Value>(&format!("/servers/{}", id), &body).await {
                    Ok(_) => {
                        new_server_name.set(String::new());
                        rename_modal_open.set(false);
                        success_msg.set("Server renamed successfully!".to_string());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_reinstall = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).post::<(), serde_json::Value>(&format!("/servers/{}/settings/reinstall", id), &()).await {
                    Ok(_) => {
                        success_msg.set("Server is being reinstalled...".to_string());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_change_egg = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let egg_id = new_egg_id.get_untracked();
            if egg_id.is_empty() { return; }

            let body = ChangeEggRequest { egg_id };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, serde_json::Value>(&format!("/servers/{}/settings/change-egg", id), &body).await {
                    Ok(_) => {
                        new_egg_id.set(String::new());
                        egg_modal_open.set(false);
                        success_msg.set("Egg changed successfully!".to_string());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_suspend = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).post::<(), serde_json::Value>(&format!("/servers/{}/settings/suspend", id), &()).await {
                    Ok(_) => {
                        suspend_confirm.set(false);
                        success_msg.set("Server suspended!".to_string());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_unsuspend = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).post::<(), serde_json::Value>(&format!("/servers/{}/settings/unsuspend", id), &()).await {
                    Ok(_) => {
                        unsuspend_confirm.set(false);
                        success_msg.set("Server unsuspended!".to_string());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    view! {
        <div class="space-y-6">
            <ErrorBanner msg=error />

            <Show when=move || !success_msg.get().is_empty()>
                <div class="p-4 bg-green-50 border border-green-200 rounded text-green-800">
                    {move || success_msg.get()}
                </div>
            </Show>

            {if is_owner || is_admin {
                view! {
                    <div class="space-y-4">
                        <h3 class="text-lg font-bold">Server Actions</h3>

                        <div class="border-l-4 border-blue-600 bg-blue-50 p-4 rounded">
                            <h4 class="font-medium mb-2">Rename Server</h4>
                            <p class="text-sm text-gray-600 mb-3">Change the name of this server</p>
                            <button
                                on:click=move |_| rename_modal_open.set(true)
                                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                            >
                                "Rename"
                            </button>
                        </div>

                        <div class="border-l-4 border-orange-600 bg-orange-50 p-4 rounded">
                            <h4 class="font-medium mb-2">Reinstall Server</h4>
                            <p class="text-sm text-gray-600 mb-3">Re-provision the server (this will reset its state)</p>
                            <button
                                on:click=move |_| reinstall_confirm.set(true)
                                class="px-4 py-2 bg-orange-600 text-white rounded hover:bg-orange-700 font-medium"
                            >
                                "Reinstall"
                            </button>
                        </div>
                    </div>

                    {if is_admin {
                        view! {
                            <div class="space-y-4">
                                <div class="border-l-4 border-purple-600 bg-purple-50 p-4 rounded">
                                    <h4 class="font-medium mb-2">Change Egg</h4>
                                    <p class="text-sm text-gray-600 mb-3">Change the egg template for this server (admin only)</p>
                                    <button
                                        on:click=move |_| egg_modal_open.set(true)
                                        class="px-4 py-2 bg-purple-600 text-white rounded hover:bg-purple-700 font-medium"
                                    >
                                        "Change Egg"
                                    </button>
                                </div>

                                <div class="border-l-4 border-red-600 bg-red-50 p-4 rounded">
                                    <h4 class="font-medium mb-2">Suspend / Unsuspend</h4>
                                    <p class="text-sm text-gray-600 mb-3">Suspend or unsuspend this server (admin only)</p>
                                    <div class="flex gap-2">
                                        <button
                                            on:click=move |_| suspend_confirm.set(true)
                                            class="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 font-medium"
                                        >
                                            "Suspend"
                                        </button>
                                        <button
                                            on:click=move |_| unsuspend_confirm.set(true)
                                            class="px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700 font-medium"
                                        >
                                            "Unsuspend"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! { }.into_any()
                    }}
                }.into_any()
            } else {
                view! {
                    <p class="text-gray-600">You do not have permission to modify server settings.</p>
                }.into_any()
            }}

            // Rename modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !rename_modal_open.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| rename_modal_open.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">Rename Server</h3>
                        <button
                            on:click=move |_| rename_modal_open.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="space-y-4">
                        <input
                            type="text"
                            placeholder="New server name"
                            value=move || new_server_name.get()
                            on:input=move |e| new_server_name.set(event_target_value(&e))
                            class="w-full px-4 py-2 border border-gray-300 rounded"
                        />
                        <div class="flex gap-2 justify-end">
                            <button
                                on:click=move |_| rename_modal_open.set(false)
                                class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=move |_| on_rename.run(())
                                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700"
                            >
                                "Rename"
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            // Change egg modal
            {if is_admin {
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !egg_modal_open.get())>
                        <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| egg_modal_open.set(false) />
                        <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                            <div class="flex justify-between items-center mb-6">
                                <h3 class="text-xl font-bold">Change Egg</h3>
                                <button
                                    on:click=move |_| egg_modal_open.set(false)
                                    class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                                >
                                    "×"
                                </button>
                            </div>
                            <div class="space-y-4">
                                <input
                                    type="text"
                                    placeholder="Egg ID"
                                    value=move || new_egg_id.get()
                                    on:input=move |e| new_egg_id.set(event_target_value(&e))
                                    class="w-full px-4 py-2 border border-gray-300 rounded"
                                />
                                <div class="flex gap-2 justify-end">
                                    <button
                                        on:click=move |_| egg_modal_open.set(false)
                                        class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        on:click=move |_| on_change_egg.run(())
                                        class="px-4 py-2 bg-purple-600 text-white rounded hover:bg-purple-700"
                                    >
                                        "Change"
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { }.into_any()
            }}

            // Suspend confirm modal
            {if is_admin {
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !suspend_confirm.get())>
                        <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| suspend_confirm.set(false) />
                        <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                            <h3 class="text-xl font-bold mb-4">Suspend Server?</h3>
                            <p class="text-gray-600 mb-6">The server will be stopped and users cannot start it.</p>
                            <div class="flex gap-2 justify-end">
                                <button
                                    on:click=move |_| suspend_confirm.set(false)
                                    class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                                >
                                    "Cancel"
                                </button>
                                <button
                                    on:click=move |_| on_suspend.run(())
                                    class="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700"
                                >
                                    "Suspend"
                                </button>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { }.into_any()
            }}

            // Unsuspend confirm modal
            {if is_admin {
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !unsuspend_confirm.get())>
                        <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| unsuspend_confirm.set(false) />
                        <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                            <h3 class="text-xl font-bold mb-4">Unsuspend Server?</h3>
                            <p class="text-gray-600 mb-6">The server will be ready to use again.</p>
                            <div class="flex gap-2 justify-end">
                                <button
                                    on:click=move |_| unsuspend_confirm.set(false)
                                    class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                                >
                                    "Cancel"
                                </button>
                                <button
                                    on:click=move |_| on_unsuspend.run(())
                                    class="px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700"
                                >
                                    "Unsuspend"
                                </button>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { }.into_any()
            }}

            // Reinstall confirm modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !reinstall_confirm.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| reinstall_confirm.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <h3 class="text-xl font-bold mb-4">Reinstall Server?</h3>
                    <p class="text-gray-600 mb-6">This will re-provision the server and reset its state. This action cannot be undone.</p>
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| reinstall_confirm.set(false)
                            class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| {
                                on_reinstall.run(());
                                reinstall_confirm.set(false);
                            }
                            class="px-4 py-2 bg-orange-600 text-white rounded hover:bg-orange-700"
                        >
                            "Reinstall"
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
