use leptos::prelude::*;
use serde::Deserialize;
use crate::api::sse::use_sse_callback;
use crate::state::SessionContext;

const API_BASE: &str = "/api";

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

    // use_sse_callback substitui o valor a cada tick — evita Vec crescendo linearmente.
    // Chamado no escopo raiz do componente para que on_cleanup tenha Reactive Owner correto.
    let url = format!(
        "{}/servers/{}/stats?token={}",
        API_BASE, server_id, session.token()
    );
    use_sse_callback(url, move |data| {
        if let Ok(s) = serde_json::from_str::<Stats>(&data) {
            stats.set(Some(s));
        }
    });

    let stats = move || stats.get();

    fn fmt_mb(bytes: u64) -> String {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    }

    view! {
        <Show
            when=move || stats().is_some()
            fallback=|| view! {
                <p class="text-gray-500 text-sm">"Waiting for stats..."</p>
            }
        >
            {move || stats().map(|s| {
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
