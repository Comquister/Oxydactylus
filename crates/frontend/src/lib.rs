use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

mod api;
mod components;
mod pages;
mod state;

use components::Navbar;
use pages::{
    admin::{
        AdminLayout,
        eggs::AdminEggsPage,
        nodes::AdminNodesPage,
        servers::AdminServersPage,
        users::AdminUsersPage,
    },
    client::{ClientLayout, dashboard::ClientDashboardPage, server_detail::ServerDetailPage},
    login::LoginPage,
    not_found::NotFoundPage,
};

#[component]
fn App() -> impl IntoView {
    let session = state::SessionContext::new();
    provide_context(session);

    view! {
        <Navbar />
        <Router>
            <Routes fallback=NotFoundPage>
                <Route path=path!("/login") view=LoginPage />
                // ParentRoute exige <Outlet /> no componente pai para injetar as sub-rotas.
                // Subcaminhos são RELATIVOS ao pai (sem barra inicial).
                <ParentRoute path=path!("/admin") view=AdminLayout>
                    <Route path=path!("users") view=AdminUsersPage />
                    <Route path=path!("nodes") view=AdminNodesPage />
                    <Route path=path!("servers") view=AdminServersPage />
                    <Route path=path!("eggs") view=AdminEggsPage />
                </ParentRoute>
                <ParentRoute path=path!("/client") view=ClientLayout>
                    <Route path=path!("") view=ClientDashboardPage />
                    <Route path=path!("servers/:id") view=ServerDetailPage />
                </ParentRoute>
                <Route path=path!("/") view=LoginPage />
            </Routes>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
