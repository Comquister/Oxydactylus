use leptos::prelude::*;
use leptos::html::Div;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::EventSource;
use crate::state::SessionContext;

const API_BASE: &str = "/api";

#[component]
pub fn LogsTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let streaming = RwSignal::new(false);
    let logs = RwSignal::new(Vec::<String>::new());

    // EventSource armazenado em signal para acesso no stop e no on_cleanup do root.
    // on_cleanup registrado aqui (escopo reativo correto) garante fechamento no desmonte,
    // mesmo que a conexão seja criada later (dentro do callback de clique).
    let event_source: RwSignal<Option<EventSource>> = RwSignal::new(None);

    on_cleanup(move || {
        if let Some(source) = event_source.get_untracked() {
            source.close();
        }
    });

    let start_stream = {
        let id = server_id.clone();
        let tok = session.token();
        Callback::new(move |_| {
            if streaming.get_untracked() { return; }
            streaming.set(true);

            // SSE não suporta headers customizados no browser — token vai via query param
            let url = format!("{}/servers/{}/logs?follow=true&token={}", API_BASE, id, tok);
            let source = EventSource::new(&url).expect("EventSource");

            let onmessage = Closure::wrap(Box::new({
                let logs = logs.clone();
                move |event: web_sys::MessageEvent| {
                    let data = event.data().as_string().unwrap_or_default();
                    logs.update(|v| v.push(data));
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);

            source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            event_source.set(Some(source));
            // onmessage é intencionalmente "esquecido" aqui pois o cleanup do component
            // root fecha o EventSource, e o JS GC cuida da função JS associada.
            // A alternativa (on_cleanup dentro de callback) perderia o Reactive Owner.
            let _ = onmessage;
        })
    };

    let stop_stream = move || {
        streaming.set(false);
        if let Some(source) = event_source.get_untracked() {
            source.close();
        }
        event_source.set(None);
        logs.set(vec![]);
    };

    let log_div: NodeRef<Div> = NodeRef::new();

    Effect::new(move |_| {
        let _ = logs.get();
        if let Some(el) = log_div.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });

    view! {
        <div class="space-y-3">
            <div class="flex gap-2">
                <button
                    on:click=move |_| start_stream.run(())
                    disabled=streaming.get()
                    class="px-4 py-2 bg-blue-600 text-white rounded disabled:opacity-50"
                >
                    "Stream Logs"
                </button>
                <button
                    on:click=move |_| stop_stream()
                    class="px-4 py-2 bg-gray-200 rounded"
                >
                    "Stop / Clear"
                </button>
            </div>
            <div
                node_ref=log_div
                class="bg-gray-950 text-gray-200 p-4 rounded-lg font-mono text-xs h-96 overflow-y-auto"
            >
                {move || logs.get().into_iter().map(|line| view! {
                    <div class="whitespace-pre-wrap leading-5">{line}</div>
                }).collect_view()}
            </div>
        </div>
    }
}
