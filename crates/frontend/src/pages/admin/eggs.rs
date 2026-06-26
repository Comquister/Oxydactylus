use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, eggs::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminEggsPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let eggs = RwSignal::new(Vec::<Egg>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let f_author = RwSignal::new(String::new());
    let f_version = RwSignal::new("1.0.0".to_string());
    let f_start = RwSignal::new(String::new());
    let f_stop = RwSignal::new("^C".to_string());
    let f_done = RwSignal::new(String::new());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Egg>>("/eggs").await {
                    Ok(v) => eggs.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };
    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/eggs/{}", id)).await {
                    error.set(e);
                } else {
                    eggs.update(|v| v.retain(|e| e.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateEggBody {
                name: f_name.get_untracked(),
                description: f_desc.get_untracked(),
                author: f_author.get_untracked(),
                version: f_version.get_untracked(),
                start_cmd: f_start.get_untracked(),
                stop_cmd: f_stop.get_untracked(),
                startup_done: f_done.get_untracked(),
                docker_images: serde_json::json!({}),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Egg>("/eggs", &body).await {
                    Ok(e) => {
                        eggs.update(|v| v.push(e));
                        show_modal.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |e: &Egg| e.name.clone() },
        Column { header: "Author", render: |e: &Egg| e.author.clone() },
        Column { header: "Version", render: |e: &Egg| e.version.clone() },
    ];

    view! {
        <Card title="Eggs".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error />
                <div class="flex justify-end">
                    <Button on_click=Callback::new(move |_| show_modal.set(true))>
                        "+ Create Egg"
                    </Button>
                </div>
                <Table columns=columns rows=eggs.get() on_delete=on_delete key_fn=|e: &Egg| e.id.clone() />
            </div>
        </Card>

        <Modal title="Create Egg".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_desc label="Description" />
                <TextInput value=f_author label="Author" />
                <TextInput value=f_version label="Version" />
                <TextInput value=f_start label="Start Command" />
                <TextInput value=f_stop label="Stop Command" />
                <TextInput value=f_done label="Startup Done (regex)" />
                <Button on_click=on_create>"Create"</Button>
            </div>
        </Modal>
    }
}
