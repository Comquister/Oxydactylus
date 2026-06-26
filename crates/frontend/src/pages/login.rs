use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen_futures::spawn_local;
use crate::api::auth::login;
use crate::components::{Button, ErrorBanner, TextInput};
use crate::state::{AuthState, SessionContext};

#[component]
pub fn LoginPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");
    let navigate = use_navigate();

    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let loading = RwSignal::new(false);

    let handle_submit = move |_: ()| {
        if email.get_untracked().is_empty() || password.get_untracked().is_empty() {
            error.set("Email e senha são obrigatórios".to_string());
            return;
        }

        error.set(String::new());
        loading.set(true);

        let navigate = navigate.clone();
        spawn_local(async move {
            let result = login(&email.get_untracked(), &password.get_untracked()).await;
            loading.set(false);

            match result {
                Ok(resp) => {
                    let is_admin = resp.is_admin;
                    session.set_auth(AuthState {
                        access_token: resp.access_token,
                        refresh_token: resp.refresh_token,
                        email: resp.email,
                        is_admin,
                    });
                    if is_admin {
                        navigate("/admin/users", Default::default());
                    } else {
                        navigate("/client", Default::default());
                    }
                }
                Err(e) => error.set(format!("Falha no login: {}", e)),
            }
        });
    };

    view! {
        <div class="flex items-center justify-center min-h-screen bg-gradient-to-br from-slate-950 to-slate-800">
            <div class="bg-white rounded-xl shadow-2xl p-8 w-full max-w-sm">
                <h1 class="text-3xl font-bold text-center text-gray-900 mb-8">"Oxydactylus"</h1>

                <form
                    class="space-y-4"
                    on:submit=move |e| {
                        e.prevent_default();
                        handle_submit(());
                    }
                >
                    <ErrorBanner msg=error />
                    <TextInput value=email placeholder="admin@example.com" label="Email" />
                    <TextInput value=password placeholder="••••••••" label="Password" input_type="password" />
                    <Button disabled=loading.get() variant="primary">
                        {move || if loading.get() { "Entrando..." } else { "Entrar" }}
                    </Button>
                </form>
            </div>
        </div>
    }
}
