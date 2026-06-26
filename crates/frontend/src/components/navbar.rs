use leptos::prelude::*;
use leptos_router::components::A;
use crate::state::SessionContext;

#[component]
pub fn Navbar() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    view! {
        <nav class="bg-slate-900 text-white shadow-lg">
            <div class="max-w-7xl mx-auto px-4 py-3 flex justify-between items-center">
                <A href="/" attr:class="text-xl font-bold tracking-tight">
                    "Oxydactylus"
                </A>
                <Show when=move || session.auth.get().is_some()>
                    <div class="flex items-center gap-6">
                        <Show when=move || session.is_admin()>
                            <A href="/admin/users" attr:class="hover:text-blue-400 text-sm">"Admin"</A>
                        </Show>
                        <A href="/client" attr:class="hover:text-blue-400 text-sm">"Servers"</A>
                        <span class="text-gray-400 text-sm">
                            {move || session.auth.get().map(|a| a.email).unwrap_or_default()}
                        </span>
                        <button
                            on:click=move |_| session.clear()
                            class="px-3 py-1 bg-red-700 hover:bg-red-800 rounded text-sm"
                        >
                            "Logout"
                        </button>
                    </div>
                </Show>
            </div>
        </nav>
    }
}
