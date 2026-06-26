use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_navigate;
use crate::state::SessionContext;

pub mod console_tab;
pub mod dashboard;
pub mod logs_tab;
pub mod server_detail;
pub mod stats_tab;

#[component]
pub fn ClientLayout() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    if session.auth.get_untracked().is_none() {
        navigate("/login", Default::default());
    }

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <Outlet />
        </div>
    }
}
