use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{StartupVariable, UpdateStartupRequest, UpdateDockerImageRequest}};
use crate::components::ErrorBanner;
use crate::state::SessionContext;
use std::collections::HashMap;

#[component]
pub fn StartupTab(server_id: String, is_admin: bool) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let variables = RwSignal::new(Vec::<StartupVariable>::new());
    let docker_image = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    let new_image = RwSignal::new(String::new());
    let show_image_modal = RwSignal::new(false);

    let tok_load = session.token();
    let id_load = server_id.clone();
    let load_startup = Callback::new(move |_: ()| {
        let tok = tok_load.clone();
        let id = id_load.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match ApiClient::new(tok).get::<Vec<StartupVariable>>(&format!("/servers/{}/startup", id)).await {
                Ok(vars) => {
                    if !vars.is_empty() {
                        docker_image.set(String::new());
                    }
                    variables.set(vars);
                },
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    spawn_local({
        let load = load_startup.clone();
        async move {
            load.run(());
        }
    });

    let on_save_variables = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let vars = variables.get_untracked();
            let mut map = HashMap::new();
            for var in vars {
                map.insert(var.env_variable, var.value);
            }
            let body = UpdateStartupRequest { variables: map };

            spawn_local(async move {
                match ApiClient::new(tok).put::<_, serde_json::Value>(&format!("/servers/{}/startup", id), &body).await {
                    Ok(_) => {
                        success_msg.set("Startup variables saved!".to_string());
                        load_startup.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_update_image = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let img = new_image.get_untracked();
            if img.is_empty() { return; }

            let body = UpdateDockerImageRequest { image: img };
            spawn_local(async move {
                match ApiClient::new(tok).put::<_, serde_json::Value>(&format!("/servers/{}/startup/image", id), &body).await {
                    Ok(_) => {
                        new_image.set(String::new());
                        show_image_modal.set(false);
                        success_msg.set("Docker image updated!".to_string());
                        load_startup.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    view! {
        <div class="space-y-4">
            <ErrorBanner msg=error />

            <Show when=move || !success_msg.get().is_empty()>
                <div class="p-4 bg-green-50 border border-green-200 rounded text-green-800">
                    {move || success_msg.get()}
                </div>
            </Show>

            <Show when=move || !loading.get()>
                <div class="space-y-4">
                    {move || {
                        let vars = variables.get();
                        if vars.is_empty() {
                            return view! {
                                <p class="text-gray-600">"No startup variables available for this server."</p>
                            }.into_any();
                        }

                        view! {
                            <div class="rounded-lg border border-gray-200 overflow-hidden">
                                <table class="w-full text-sm text-left">
                                    <thead class="bg-gray-50">
                                        <tr>
                                            <th class="px-4 py-3 font-semibold">Name</th>
                                            <th class="px-4 py-3 font-semibold">Value</th>
                                            <th class="px-4 py-3 font-semibold">Default</th>
                                        </tr>
                                    </thead>
                                    <tbody class="divide-y divide-gray-100">
                                        {vars.into_iter().map(|var| {
                                            let var_clone = var.clone();
                                            let env_var = var.env_variable.clone();
                                            view! {
                                                <tr class="hover:bg-gray-50">
                                                    <td class="px-4 py-3">
                                                        <div>
                                                            <p class="font-medium">{var.name.clone()}</p>
                                                            {var.description.clone().map(|desc| {
                                                                view! {
                                                                    <p class="text-xs text-gray-500">{desc}</p>
                                                                }
                                                            })}
                                                            <p class="text-xs text-gray-400">{var.env_variable.clone()}</p>
                                                        </div>
                                                    </td>
                                                    <td class="px-4 py-3">
                                                        {if var_clone.user_editable {
                                                            view! {
                                                                <input
                                                                    type="text"
                                                                    value=var_clone.value.clone()
                                                                    on:input=move |e| {
                                                                        let new_val = event_target_value(&e);
                                                                        variables.update(|vars| {
                                                                            if let Some(v) = vars.iter_mut().find(|v| v.env_variable == env_var) {
                                                                                v.value = new_val;
                                                                            }
                                                                        });
                                                                    }
                                                                    class="w-full px-3 py-2 border border-gray-300 rounded"
                                                                />
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <p class="text-gray-600">{var_clone.value}</p>
                                                            }.into_any()
                                                        }}
                                                    </td>
                                                    <td class="px-4 py-3 text-gray-600">
                                                        {var_clone.default_val.unwrap_or_else(|| "—".to_string())}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>

                            <div class="flex gap-2">
                                <button
                                    on:click=move |_| on_save_variables.run(())
                                    class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                                >
                                    "Save Variables"
                                </button>
                                {if is_admin {
                                    view! {
                                        <button
                                            on:click=move |_| show_image_modal.set(true)
                                            class="px-4 py-2 bg-gray-600 text-white rounded hover:bg-gray-700 font-medium"
                                        >
                                            "Change Docker Image"
                                        </button>
                                    }.into_any()
                                } else {
                                    view! { }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }}
                </div>
            </Show>

            {if is_admin {
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !show_image_modal.get())>
                        <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| show_image_modal.set(false) />
                        <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                            <div class="flex justify-between items-center mb-6">
                                <h3 class="text-xl font-bold">Change Docker Image</h3>
                                <button
                                    on:click=move |_| show_image_modal.set(false)
                                    class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                                >
                                    "×"
                                </button>
                            </div>
                            <div class="space-y-4">
                                <input
                                    type="text"
                                    placeholder="e.g., ghcr.io/pterodactyl/yolks:java_17"
                                    value=move || new_image.get()
                                    on:input=move |e| new_image.set(event_target_value(&e))
                                    class="w-full px-4 py-2 border border-gray-300 rounded"
                                />
                                <div class="flex gap-2 justify-end">
                                    <button
                                        on:click=move |_| show_image_modal.set(false)
                                        class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        on:click=move |_| on_update_image.run(())
                                        class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700"
                                    >
                                        "Update"
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { }.into_any()
            }}
        </div>
    }
}
