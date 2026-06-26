use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::Server};
use crate::components::{Card, ErrorBanner};
use crate::state::SessionContext;

#[component]
pub fn ClientDashboardPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let servers = RwSignal::new(Vec::<Server>::new());
    let error = RwSignal::new(String::new());

    {
        let tok = session.token();
        spawn_local(async move {
            match ApiClient::new(tok).get::<Vec<Server>>("/servers").await {
                Ok(v) => servers.set(v),
                Err(e) => error.set(e),
            }
        });
    }

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold text-gray-900">"My Servers"</h1>
            <ErrorBanner msg=error />
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {move || servers.get().into_iter().map(|srv| {
                    let status_color = match srv.status.as_str() {
                        "running" => "text-green-600",
                        "stopped" => "text-gray-500",
                        "error" => "text-red-600",
                        _ => "text-yellow-600",
                    };
                    view! {
                        <Card title=srv.name.clone() subtitle=format!("{}MB RAM | {}% CPU", srv.memory_mb, srv.cpu_percent)>
                            <div class="space-y-3">
                                <p class=format!("text-sm font-semibold {}", status_color)>
                                    {srv.status.to_uppercase()}
                                </p>
                                <p class="text-xs text-gray-500 font-mono">{srv.image.clone()}</p>
                                <A
                                    href=format!("/client/servers/{}", srv.id)
                                    attr:class="inline-block px-4 py-2 bg-gray-200 hover:bg-gray-300 text-gray-800 rounded font-medium transition-colors text-sm"
                                >
                                    "Manage →"
                                </A>
                            </div>
                        </Card>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
