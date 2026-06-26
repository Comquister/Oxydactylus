use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use crate::api::client::ApiClient;
use crate::state::SessionContext;

#[derive(Clone, Deserialize)]
struct Stats {
    memory_bytes: u64,
    cpu_percent: f32,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[component]
pub fn StatsTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let stats = RwSignal::new(None::<Stats>);
    let running = RwSignal::new(true);

    on_cleanup(move || running.set(false));

    let tok = session.token();
    let id = server_id.clone();
    spawn_local(async move {
        while running.get_untracked() {
            if let Ok(s) = ApiClient::new(tok.clone()).get::<Stats>(&format!("/servers/{}/stats", id)).await {
                stats.set(Some(s));
            }
            TimeoutFuture::new(3_000).await;
        }
    });

    fn fmt_mb(bytes: u64) -> String {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    }

    view! {
        <Show
            when=move || stats.get().is_some()
            fallback=|| view! {
                <p class="text-gray-500 text-sm">"Waiting for stats..."</p>
            }
        >
            {move || stats.get().map(|s| {
                view! {
                    <div class="grid grid-cols-2 gap-4">
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Memory"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.memory_bytes)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"CPU"</p>
                            <p class="text-2xl font-bold mt-1">{format!("{:.1}%", s.cpu_percent)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Network RX"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.rx_bytes)}</p>
                        </div>
                        <div class="bg-white p-4 rounded-lg shadow border">
                            <p class="text-xs text-gray-500 uppercase tracking-wider">"Network TX"</p>
                            <p class="text-2xl font-bold mt-1">{fmt_mb(s.tx_bytes)}</p>
                        </div>
                    </div>
                }
            })}
        </Show>
    }
}
