use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::Server};
use crate::components::{Button, ErrorBanner};
use crate::state::SessionContext;
use super::{
    console_tab::ConsoleTab, files_tab::FilesTab, logs_tab::LogsTab, stats_tab::StatsTab,
    startup_tab::StartupTab, databases_tab::DatabasesTab, schedules_tab::SchedulesTab,
    backups_tab::BackupsTab, settings_tab::SettingsTab,
};

#[component]
pub fn ServerDetailPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let params = use_params_map();
    let server_id = move || params.with(|p| p.get("id").map(|s| s.to_string()).unwrap_or_default());

    let server = RwSignal::new(None::<Server>);
    let error = RwSignal::new(String::new());
    let active_tab = RwSignal::new("console");
    let user_is_admin = RwSignal::new(false);
    let user_is_owner = RwSignal::new(false);

    {
        let tok = session.token();
        let id = server_id();
        let is_admin = session.is_admin();
        user_is_admin.set(is_admin);
        user_is_owner.set(is_admin);
        spawn_local(async move {
            match ApiClient::new(tok).get::<Server>(&format!("/servers/{}", id)).await {
                Ok(s) => {
                    server.set(Some(s));
                },
                Err(e) => error.set(e),
            }
        });
    }

    let tok_start = session.token();
    let id_start = server_id();
    let on_start = Callback::new(move |_: ()| {
        let tok = tok_start.clone();
        let id = id_start.clone();
        spawn_local(async move {
            if let Err(e) = ApiClient::new(tok).post::<(), ()>(&format!("/servers/{}/start", id), &()).await {
                error.set(e);
            }
        });
    });

    let tok_stop = session.token();
    let id_stop = server_id();
    let on_stop = Callback::new(move |_: ()| {
        let tok = tok_stop.clone();
        let id = id_stop.clone();
        spawn_local(async move {
            if let Err(e) = ApiClient::new(tok).post::<(), ()>(&format!("/servers/{}/stop", id), &()).await {
                error.set(e);
            }
        });
    });

    let tok_restart = session.token();
    let id_restart = server_id();
    let on_restart = Callback::new(move |_: ()| {
        let tok = tok_restart.clone();
        let id = id_restart.clone();
        spawn_local(async move {
            if let Err(e) = ApiClient::new(tok).post::<(), ()>(&format!("/servers/{}/restart", id), &()).await {
                error.set(e);
            }
        });
    });

    view! {
        <div class="space-y-6">
            <ErrorBanner msg=error />

            <Show when=move || server.get().is_some()>
                {move || server.get().map(|srv| {
                    view! {
                        <div class="flex items-center justify-between">
                            <div>
                                <h1 class="text-3xl font-bold text-gray-900">{srv.name.clone()}</h1>
                                <p class="text-sm text-gray-500">{srv.status.clone()}</p>
                            </div>
                            <div class="flex gap-2">
                                <Button on_click=on_start>"Start"</Button>
                                <Button variant="secondary" on_click=on_stop>"Stop"</Button>
                                <Button variant="secondary" on_click=on_restart>"Restart"</Button>
                            </div>
                        </div>

                        <div class="flex gap-2 border-b border-gray-200 overflow-x-auto">
                            {["console", "logs", "stats", "files", "startup", "databases", "schedules", "backups", "settings"].map(|tab| {
                                view! {
                                    <button
                                        class=move || {
                                            if active_tab.get() == tab {
                                                "px-4 py-2 border-b-2 border-blue-600 text-blue-600 font-medium whitespace-nowrap"
                                            } else {
                                                "px-4 py-2 text-gray-500 hover:text-gray-700 whitespace-nowrap"
                                            }
                                        }
                                        on:click=move |_| active_tab.set(tab)
                                    >
                                        {match tab {
                                            "console" => "Console",
                                            "logs" => "Logs",
                                            "stats" => "Stats",
                                            "files" => "Files",
                                            "startup" => "Startup",
                                            "databases" => "Databases",
                                            "schedules" => "Schedules",
                                            "backups" => "Backups",
                                            "settings" => "Settings",
                                            _ => tab,
                                        }}
                                    </button>
                                }
                            }).into_iter().collect_view()}
                        </div>

                        <div>
                            {move || match active_tab.get() {
                                "files" => view! { <FilesTab server_id=srv.id.clone() /> }.into_any(),
                                "logs" => view! { <LogsTab server_id=srv.id.clone() /> }.into_any(),
                                "stats" => view! { <StatsTab server_id=srv.id.clone() /> }.into_any(),
                                "startup" => view! { <StartupTab server_id=srv.id.clone() is_admin=user_is_admin.get() /> }.into_any(),
                                "databases" => view! { <DatabasesTab server_id=srv.id.clone() /> }.into_any(),
                                "schedules" => view! { <SchedulesTab server_id=srv.id.clone() /> }.into_any(),
                                "backups" => view! { <BackupsTab server_id=srv.id.clone() /> }.into_any(),
                                "settings" => view! { <SettingsTab server_id=srv.id.clone() is_admin=user_is_admin.get() is_owner=user_is_owner.get() /> }.into_any(),
                                _ => view! { <ConsoleTab server_id=srv.id.clone() /> }.into_any(),
                            }}
                        </div>
                    }
                })}
            </Show>
        </div>
    }
}
