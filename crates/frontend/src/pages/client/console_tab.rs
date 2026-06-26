use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::client::ApiClient;
use crate::components::{Button, TextInput};
use crate::state::SessionContext;
use serde::Serialize;

#[derive(Serialize)]
struct CommandBody {
    content: String,
}

#[component]
pub fn ConsoleTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let cmd = RwSignal::new(String::new());
    let output = RwSignal::new(Vec::<String>::new());

    let on_send = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_| {
            let tok = tok.clone();
            let id = id.clone();
            let content = cmd.get_untracked();
            if content.is_empty() { return; }

            cmd.set(String::new());
            output.update(|v| v.push(format!("> {}", content)));

            spawn_local(async move {
                let body = CommandBody { content };
                if let Err(e) = ApiClient::new(tok)
                    .post::<_, serde_json::Value>(&format!("/servers/{}/command", id), &body)
                    .await
                {
                    output.update(|v| v.push(format!("Error: {}", e)));
                }
            });
        })
    };

    view! {
        <div class="space-y-3">
            <div class="bg-gray-950 text-gray-200 p-4 rounded-lg font-mono text-sm h-80 overflow-y-auto">
                {move || output.get().into_iter().map(|line| view! {
                    <div>{line}</div>
                }).collect_view()}
            </div>
            <form
                class="flex gap-2"
                on:submit=move |e| {
                    e.prevent_default();
                    on_send.run(());
                }
            >
                <TextInput value=cmd placeholder="Enter command..." />
                <Button>"Send"</Button>
            </form>
        </div>
    }
}
