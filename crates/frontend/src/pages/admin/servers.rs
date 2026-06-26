use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminServersPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let servers = RwSignal::new(Vec::<Server>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_image = RwSignal::new(String::new());
    let f_node = RwSignal::new(String::new());
    let f_user = RwSignal::new(String::new());
    let f_mem = RwSignal::new("512".to_string());
    let f_cpu = RwSignal::new("100".to_string());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Server>>("/servers").await {
                    Ok(v) => servers.set(v),
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
                if let Err(e) = ApiClient::new(tok).delete(&format!("/servers/{}", id)).await {
                    error.set(e);
                } else {
                    servers.update(|v| v.retain(|s| s.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateServerBody {
                user_id: f_user.get_untracked(),
                node_id: f_node.get_untracked(),
                name: f_name.get_untracked(),
                image: f_image.get_untracked(),
                memory_mb: f_mem.get_untracked().parse().unwrap_or(512),
                cpu_percent: f_cpu.get_untracked().parse().unwrap_or(100),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Server>("/servers", &body).await {
                    Ok(s) => {
                        servers.update(|v| v.push(s));
                        show_modal.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |s: &Server| s.name.clone() },
        Column { header: "Status", render: |s: &Server| s.status.clone() },
        Column { header: "Image", render: |s: &Server| s.image.clone() },
        Column { header: "RAM", render: |s: &Server| format!("{}MB", s.memory_mb) },
    ];

    view! {
        <Card title="Servers".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error />
                <div class="flex justify-end">
                    <Button on_click=Callback::new(move |_| show_modal.set(true))>
                        "+ Create Server"
                    </Button>
                </div>
                <Table columns=columns rows=servers.get() on_delete=on_delete key_fn=|s: &Server| s.id.clone() />
            </div>
        </Card>

        <Modal title="Create Server".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_image label="Docker Image" placeholder="ghcr.io/..." />
                <TextInput value=f_node label="Node ID" />
                <TextInput value=f_user label="User ID" />
                <TextInput value=f_mem label="Memory (MB)" />
                <TextInput value=f_cpu label="CPU (%)" />
                <Button on_click=on_create>"Create"</Button>
            </div>
        </Modal>
    }
}
