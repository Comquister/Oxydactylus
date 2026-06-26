use leptos::prelude::*;
use leptos_router::components::{A, Outlet};
use leptos_router::hooks::use_navigate;
use crate::state::SessionContext;

pub mod eggs;
pub mod nodes;
pub mod servers;
pub mod users;

#[component]
pub fn AdminLayout() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    if !session.is_admin() {
        navigate("/login", Default::default());
    }

    view! {
        <div class="flex min-h-screen bg-gray-50">
            <aside class="w-56 bg-white shadow-md flex-shrink-0">
                <div class="p-6">
                    <h3 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4">
                        "Administration"
                    </h3>
                    <nav class="space-y-1">
                        <A
                            href="/admin/users"
                            attr:class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                        >
                            "Users"
                        </A>
                        <A
                            href="/admin/nodes"
                            attr:class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                        >
                            "Nodes"
                        </A>
                        <A
                            href="/admin/servers"
                            attr:class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                        >
                            "Servers"
                        </A>
                        <A
                            href="/admin/eggs"
                            attr:class="flex items-center px-3 py-2 text-sm rounded-md hover:bg-gray-100 text-gray-700"
                        >
                            "Eggs"
                        </A>
                    </nav>
                </div>
            </aside>
            <main class="flex-1 p-8">
                <Outlet />
            </main>
        </div>
    }
}
