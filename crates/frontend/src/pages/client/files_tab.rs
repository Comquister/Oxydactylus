use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{FileInfo, WriteFileRequest, CreateDirectoryRequest, DeleteFilesRequest, RenameFileRequest}};
use crate::components::{ErrorBanner, Modal, TextInput};
use crate::state::SessionContext;

#[component]
pub fn FilesTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let current_dir = RwSignal::new("/".to_string());
    let files = RwSignal::new(Vec::<FileInfo>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    // Modals
    let view_modal_open = RwSignal::new(false);
    let create_dir_modal_open = RwSignal::new(false);
    let rename_modal_open = RwSignal::new(false);
    let delete_modal_open = RwSignal::new(false);

    // Modal states
    let viewed_file: RwSignal<Option<String>> = RwSignal::new(None);
    let viewed_content = RwSignal::new(String::new());
    let new_dir_name = RwSignal::new(String::new());
    let rename_old_path = RwSignal::new(String::new());
    let rename_new_name = RwSignal::new(String::new());
    let delete_path = RwSignal::new(String::new());

    // Simple URL encoding helper
    fn encode_path(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '/' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect()
    }

    // Load files when directory changes
    let tok_files = session.token();
    let id_files = server_id.clone();
    let load_files = Callback::new(move |dir: String| {
        let tok = tok_files.clone();
        let id = id_files.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match ApiClient::new(tok).get::<Vec<FileInfo>>(&format!("/servers/{}/files?directory={}", id, encode_path(&dir))).await {
                Ok(f) => {
                    files.set(f);
                    current_dir.set(dir);
                },
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    // Load on mount
    spawn_local({
        let load = load_files.clone();
        async move {
            load.run("/".to_string());
        }
    });

    // View file contents
    let on_view = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |file_path: String| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get_text(&format!("/servers/{}/files/contents?file={}", id, encode_path(&file_path))).await {
                    Ok(content) => {
                        viewed_file.set(Some(file_path));
                        viewed_content.set(content);
                        view_modal_open.set(true);
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    // Save file contents
    let on_save = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let file_path = viewed_file.get_untracked();
            let content = viewed_content.get_untracked();
            if let Some(path) = file_path {
                spawn_local(async move {
                    let body = WriteFileRequest { content };
                    match ApiClient::new(tok)
                        .post::<_, serde_json::Value>(&format!("/servers/{}/files/contents?file={}", id, encode_path(&path)), &body)
                        .await
                    {
                        Ok(_) => {
                            error.set("File saved successfully".to_string());
                            view_modal_open.set(false);
                            load_files.run(current_dir.get_untracked());
                        },
                        Err(e) => error.set(e),
                    }
                });
            }
        })
    };

    // Create directory
    let on_create_dir = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let dir_name = new_dir_name.get_untracked();
            if dir_name.is_empty() { return; }

            let current = current_dir.get_untracked();
            let path = if current.ends_with('/') {
                format!("{}{}", current, dir_name)
            } else {
                format!("{}/{}", current, dir_name)
            };

            spawn_local(async move {
                let body = CreateDirectoryRequest { path };
                match ApiClient::new(tok)
                    .post::<_, serde_json::Value>(&format!("/servers/{}/files/create-directory", id), &body)
                    .await
                {
                    Ok(_) => {
                        new_dir_name.set(String::new());
                        create_dir_modal_open.set(false);
                        load_files.run(current_dir.get_untracked());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    // Delete file/directory
    let on_delete = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let path = delete_path.get_untracked();
            if path.is_empty() { return; }

            spawn_local(async move {
                let body = DeleteFilesRequest { path, recursive: false };
                match ApiClient::new(tok)
                    .post::<_, serde_json::Value>(&format!("/servers/{}/files/delete", id), &body)
                    .await
                {
                    Ok(_) => {
                        delete_modal_open.set(false);
                        delete_path.set(String::new());
                        load_files.run(current_dir.get_untracked());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    // Rename file
    let on_rename = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let old_path = rename_old_path.get_untracked();
            let new_name = rename_new_name.get_untracked();
            if old_path.is_empty() || new_name.is_empty() { return; }

            let new_path = if let Some(last_slash) = old_path.rfind('/') {
                format!("{}/{}", &old_path[..last_slash], new_name)
            } else {
                new_name
            };

            spawn_local(async move {
                let body = RenameFileRequest { old_path, new_path };
                match ApiClient::new(tok)
                    .put::<_, serde_json::Value>(&format!("/servers/{}/files/rename", id), &body)
                    .await
                {
                    Ok(_) => {
                        rename_modal_open.set(false);
                        rename_old_path.set(String::new());
                        rename_new_name.set(String::new());
                        load_files.run(current_dir.get_untracked());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let format_size = |bytes: i64| {
        if bytes < 1024 {
            format!("{}B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        }
    };

    view! {
        <div class="space-y-4">
            <ErrorBanner msg=error />

            <div class="flex gap-2">
                <button
                    on:click=move |_| {
                        let current = current_dir.get_untracked();
                        if current != "/" {
                            if let Some(last_slash) = current.rfind('/') {
                                let parent = if last_slash == 0 {
                                    "/".to_string()
                                } else {
                                    current[..last_slash].to_string()
                                };
                                load_files.run(parent);
                            }
                        }
                    }
                    disabled=move || current_dir.get() == "/"
                    class="px-4 py-2 bg-gray-600 text-white rounded disabled:opacity-50"
                >
                    "Parent Directory"
                </button>
                <button
                    on:click=move |_| create_dir_modal_open.set(true)
                    class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                >
                    "New Folder"
                </button>
            </div>

            <div class="text-sm text-gray-600">
                "Path: " {move || current_dir.get()}
            </div>

            <div class="rounded-lg border border-gray-200 overflow-hidden">
                <table class="w-full text-sm text-left">
                    <thead class="bg-gray-50">
                        <tr>
                            <th class="px-4 py-3 font-semibold">Name</th>
                            <th class="px-4 py-3 font-semibold">Type</th>
                            <th class="px-4 py-3 font-semibold">Size</th>
                            <th class="px-4 py-3 font-semibold">Actions</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-gray-100">
                        {move || files.get().into_iter().map(|file| {
                            let file_path = file.path.clone();
                            let file_path_view = file_path.clone();
                            let file_path_rename = file_path.clone();
                            let file_path_delete = file_path.clone();
                            view! {
                                <tr class="hover:bg-gray-50">
                                    <td class="px-4 py-3">
                                        {if file.is_dir {
                                            let file_path_dir = file.path.clone();
                                            view! {
                                                <button
                                                    on:click=move |_| load_files.run(file_path_dir.clone())
                                                    class="text-blue-600 hover:underline font-medium"
                                                >
                                                    {file.name.clone()} "/"
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span>{file.name.clone()}</span>
                                            }.into_any()
                                        }}
                                    </td>
                                    <td class="px-4 py-3 text-gray-600">
                                        {if file.is_dir { "Directory" } else { "File" }}
                                    </td>
                                    <td class="px-4 py-3 text-gray-600">
                                        {format_size(file.size_bytes)}
                                    </td>
                                    <td class="px-4 py-3 space-x-2">
                                        {(!file.is_dir).then(|| {
                                            let path = file_path_view.clone();
                                            view! {
                                                <button
                                                    on:click=move |_| on_view.run(path.clone())
                                                    class="text-blue-600 hover:text-blue-800 text-sm"
                                                >
                                                    "View/Edit"
                                                </button>
                                            }
                                        }).into_any()}
                                        <button
                                            on:click=move |_| {
                                                rename_old_path.set(file_path_rename.clone());
                                                rename_modal_open.set(true);
                                            }
                                            class="text-blue-600 hover:text-blue-800 text-sm"
                                        >
                                            "Rename"
                                        </button>
                                        <button
                                            on:click=move |_| {
                                                delete_path.set(file_path_delete.clone());
                                                delete_modal_open.set(true);
                                            }
                                            class="text-red-600 hover:text-red-800 text-sm"
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

            <Show when=move || loading.get()>
                <div class="text-center text-gray-600">"Loading..."</div>
            </Show>

            <Modal title="View/Edit File" open=view_modal_open>
                <div class="space-y-4">
                    <p class="text-xs text-gray-600">{move || viewed_file.get().unwrap_or_default()}</p>
                    <textarea
                        prop:value=move || viewed_content.get()
                        on:input=move |ev| viewed_content.set(event_target_value(&ev))
                        class="w-full h-64 p-2 border border-gray-300 rounded font-mono text-sm"
                        placeholder="File contents..."
                    />
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| view_modal_open.set(false)
                            class="px-4 py-2 bg-gray-300 rounded hover:bg-gray-400"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| on_save.run(())
                            class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                        >
                            "Save"
                        </button>
                    </div>
                </div>
            </Modal>

            <Modal title="Create Directory" open=create_dir_modal_open>
                <div class="space-y-4">
                    <TextInput value=new_dir_name placeholder="Directory name..." />
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| create_dir_modal_open.set(false)
                            class="px-4 py-2 bg-gray-300 rounded hover:bg-gray-400"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| on_create_dir.run(())
                            class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                        >
                            "Create"
                        </button>
                    </div>
                </div>
            </Modal>

            <Modal title="Rename" open=rename_modal_open>
                <div class="space-y-4">
                    <p class="text-xs text-gray-600">{move || rename_old_path.get()}</p>
                    <TextInput value=rename_new_name placeholder="New name..." />
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| rename_modal_open.set(false)
                            class="px-4 py-2 bg-gray-300 rounded hover:bg-gray-400"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| on_rename.run(())
                            class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
                        >
                            "Rename"
                        </button>
                    </div>
                </div>
            </Modal>

            <Modal title="Delete Confirmation" open=delete_modal_open>
                <div class="space-y-4">
                    <p class="text-gray-700">
                        "Are you sure you want to delete "
                        <code class="bg-gray-100 px-1 rounded">{move || delete_path.get()}</code>
                        "?"
                    </p>
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| delete_modal_open.set(false)
                            class="px-4 py-2 bg-gray-300 rounded hover:bg-gray-400"
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
            </Modal>
        </div>
    }
}
