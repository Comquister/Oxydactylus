use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, users::*};
use crate::components::{Button, Card, Column, ErrorBanner, Modal, Table, TextInput};
use crate::state::SessionContext;

#[component]
pub fn AdminUsersPage() -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let users = RwSignal::new(Vec::<User>::new());
    let error = RwSignal::new(String::new());
    let show_modal = RwSignal::new(false);

    let f_email = RwSignal::new(String::new());
    let f_password = RwSignal::new(String::new());
    let f_admin = RwSignal::new(false);

    let load = {
        let tok = session.token();
        move || {
            let tok = tok.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<User>>("/users").await {
                    Ok(v) => users.set(v),
                    Err(e) => error.set(e),
                }
            });
        }
    };

    load();

    let on_delete = Callback::new({
        let tok = session.token();
        move |id: String| {
            let tok = tok.clone();
            spawn_local(async move {
                if let Err(e) = ApiClient::new(tok).delete(&format!("/users/{}", id)).await {
                    error.set(e);
                } else {
                    users.update(|v| v.retain(|u| u.id != id));
                }
            });
        }
    });

    let on_create = Callback::new({
        let tok = session.token();
        move |_| {
            let tok = tok.clone();
            let body = CreateUserBody {
                email: f_email.get_untracked(),
                password: f_password.get_untracked(),
                is_admin: f_admin.get_untracked(),
            };
            spawn_local(async move {
                match ApiClient::new(tok).post::<_, User>("/users", &body).await {
                    Ok(u) => {
                        users.update(|v| v.push(u));
                        show_modal.set(false);
                        f_email.set(String::new());
                        f_password.set(String::new());
                        f_admin.set(false);
                    }
                    Err(e) => error.set(e),
                }
            });
        }
    });

    let columns = vec![
        Column { header: "Email", render: |u: &User| u.email.clone() },
        Column { header: "Admin", render: |u: &User| if u.is_admin { "Sim".into() } else { "Não".into() } },
        Column { header: "Criado em", render: |u: &User| u.created_at.clone() },
    ];

    view! {
        <Card title="Users".to_string()>
            <div class="space-y-4">
                <ErrorBanner msg=error />
                <div class="flex justify-end">
                    <Button on_click=Callback::new(move |_| show_modal.set(true))>
                        "+ Create User"
                    </Button>
                </div>
                <Table columns=columns rows=users.get() on_delete=on_delete key_fn=|u: &User| u.id.clone() />
            </div>
        </Card>

        <Modal title="Create User".to_string() open=show_modal>
            <div class="space-y-4">
                <TextInput value=f_email label="Email" />
                <TextInput value=f_password label="Password" input_type="password" />
                <label class="flex items-center gap-2 text-sm">
                    <input
                        type="checkbox"
                        prop:checked=f_admin
                        on:change=move |ev| f_admin.set(event_target_checked(&ev))
                    />
                    "Is Admin"
                </label>
                <Button on_click=on_create>"Create"</Button>
            </div>
        </Modal>
    }
}
