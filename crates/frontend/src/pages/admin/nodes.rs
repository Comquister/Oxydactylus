use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, nodes::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminNodesPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let nodes = RwSignal::new(Vec::<Node>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_addr = RwSignal::new(String::new());

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<Node>>("/nodes").await {
                    Ok(v) => nodes.set(v),
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
                if let Err(e) = ApiClient::new(tok).delete(&format!("/nodes/{}", id)).await {
                    error.set(e);
                } else {
                    nodes.update(|v| v.retain(|n| n.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateNodeBody {
                name: f_name.get_untracked(),
                grpc_addr: f_addr.get_untracked(),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, Node>("/nodes", &body).await {
                    Ok(n) => {
                        nodes.update(|v| v.push(n));
                        show_modal.set(false);
                        f_name.set(String::new());
                        f_addr.set(String::new());
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Name", render: |n: &Node| n.name.clone() },
        Column { header: "gRPC Address", render: |n: &Node| n.grpc_addr.clone() },
        Column { header: "Criado em", render: |n: &Node| n.created_at.clone() },
    ];

    view! {
        <Card title="Nodes".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error />
                <div class="flex justify-end">
                    <Button on_click=Callback::new(move |_| show_modal.set(true))>
                        "+ Create Node"
                    </Button>
                </div>
                <Table columns=columns rows=nodes.get() on_delete=on_delete key_fn=|n: &Node| n.id.clone() />
            </div>
        </Card>

        <Modal title="Create Node".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_name label="Name" />
                <TextInput value=f_addr label="gRPC Address" placeholder="host:50051" />
                <Button on_click=on_create>"Create"</Button>
            </div>
        </Modal>
    }
}
