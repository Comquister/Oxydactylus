use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::api::{client::ApiClient, servers::{ScheduleInfo, ScheduleTask, CreateScheduleRequest, CreateScheduleTaskRequest}};
use crate::components::{ErrorBanner};
use crate::state::SessionContext;

#[component]
pub fn SchedulesTab(server_id: String) -> impl IntoView {
    let session = use_context::<SessionContext>().expect("SessionContext");

    let schedules = RwSignal::new(Vec::<ScheduleInfo>::new());
    let selected_schedule = RwSignal::new(None::<String>);
    let schedule_tasks = RwSignal::new(Vec::<ScheduleTask>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let create_schedule_modal = RwSignal::new(false);
    let delete_schedule_modal = RwSignal::new(false);
    let add_task_modal = RwSignal::new(false);

    let sched_name = RwSignal::new(String::new());
    let sched_minute = RwSignal::new("0".to_string());
    let sched_hour = RwSignal::new("0".to_string());
    let sched_day_of_month = RwSignal::new("*".to_string());
    let sched_month = RwSignal::new("*".to_string());
    let sched_day_of_week = RwSignal::new("*".to_string());

    let task_action = RwSignal::new("command".to_string());
    let task_payload = RwSignal::new(String::new());
    let task_time_offset = RwSignal::new(String::new());

    let tok_load = session.token();
    let id_load = server_id.clone();
    let load_schedules = Callback::new(move |_: ()| {
        let tok = tok_load.clone();
        let id = id_load.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match ApiClient::new(tok).get::<Vec<ScheduleInfo>>(&format!("/servers/{}/schedules", id)).await {
                Ok(scheds) => schedules.set(scheds),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    spawn_local({
        let load = load_schedules.clone();
        async move {
            load.run(());
        }
    });

    let load_tasks = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |schedule_id: String| {
            let tok = tok.clone();
            let id = id.clone();
            spawn_local(async move {
                match ApiClient::new(tok).get::<Vec<ScheduleTask>>(&format!("/servers/{}/schedules/{}/tasks", id, schedule_id)).await {
                    Ok(tasks) => schedule_tasks.set(tasks),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_create_schedule = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let name = sched_name.get_untracked();
            if name.is_empty() { return; }

            let body = CreateScheduleRequest {
                name,
                cron_minute: {
                    let m = sched_minute.get_untracked();
                    if m.is_empty() { None } else { Some(m) }
                },
                cron_hour: {
                    let h = sched_hour.get_untracked();
                    if h.is_empty() { None } else { Some(h) }
                },
                cron_day_of_month: {
                    let d = sched_day_of_month.get_untracked();
                    if d.is_empty() { None } else { Some(d) }
                },
                cron_month: {
                    let m = sched_month.get_untracked();
                    if m.is_empty() { None } else { Some(m) }
                },
                cron_day_of_week: {
                    let w = sched_day_of_week.get_untracked();
                    if w.is_empty() { None } else { Some(w) }
                },
                only_when_online: None,
            };

            spawn_local(async move {
                match ApiClient::new(tok).post::<_, ScheduleInfo>(&format!("/servers/{}/schedules", id), &body).await {
                    Ok(_) => {
                        sched_name.set(String::new());
                        sched_minute.set("0".to_string());
                        sched_hour.set("0".to_string());
                        sched_day_of_month.set("*".to_string());
                        sched_month.set("*".to_string());
                        sched_day_of_week.set("*".to_string());
                        create_schedule_modal.set(false);
                        load_schedules.run(());
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let on_delete_schedule = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let sched_id = selected_schedule.get_untracked();
            if let Some(sched_id) = sched_id {
                spawn_local(async move {
                    match ApiClient::new(tok).delete(&format!("/servers/{}/schedules/{}", id, sched_id)).await {
                        Ok(_) => {
                            selected_schedule.set(None);
                            schedule_tasks.set(Vec::new());
                            delete_schedule_modal.set(false);
                            load_schedules.run(());
                        },
                        Err(e) => error.set(e),
                    }
                });
            }
        })
    };

    let on_add_task = {
        let tok = session.token();
        let id = server_id.clone();
        Callback::new(move |_: ()| {
            let tok = tok.clone();
            let id = id.clone();
            let sched_id = selected_schedule.get_untracked();
            let action = task_action.get_untracked();
            let payload = task_payload.get_untracked();
            if sched_id.is_none() || action.is_empty() || payload.is_empty() { return; }

            let body = CreateScheduleTaskRequest {
                action,
                payload,
                time_offset: task_time_offset.get_untracked().parse().ok(),
                continue_on_failure: None,
            };

            spawn_local(async move {
                if let Some(sched_id) = sched_id {
                    match ApiClient::new(tok).post::<_, ScheduleTask>(&format!("/servers/{}/schedules/{}/tasks", id, sched_id), &body).await {
                        Ok(_) => {
                            task_action.set("command".to_string());
                            task_payload.set(String::new());
                            task_time_offset.set(String::new());
                            add_task_modal.set(false);
                            load_tasks.run(sched_id);
                        },
                        Err(e) => error.set(e),
                    }
                }
            });
        })
    };

    view! {
        <div class="space-y-4">
            <ErrorBanner msg=error />

            <button
                on:click=move |_| create_schedule_modal.set(true)
                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 font-medium"
            >
                "Create Schedule"
            </button>

            <Show when=move || !loading.get()>
                {move || {
                    let scheds = schedules.get();
                    if scheds.is_empty() {
                        return view! {
                            <p class="text-gray-600">"No schedules created yet."</p>
                        }.into_any();
                    }

                    view! {
                        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
                            <div>
                                <h3 class="text-lg font-bold mb-2">Schedules</h3>
                                <div class="space-y-2">
                                    {scheds.into_iter().map(|sched| {
                                        let sched_id = sched.id.clone();
                                        view! {
                                            <div
                                                on:click=move |_| {
                                                    selected_schedule.set(Some(sched_id.clone()));
                                                    load_tasks.run(sched_id.clone());
                                                }
                                                class=move || {
                                                    let is_selected = selected_schedule.get().map(|id| id == sched.id.clone()).unwrap_or(false);
                                                    if is_selected {
                                                        "p-3 bg-blue-50 border-l-4 border-blue-600 cursor-pointer"
                                                    } else {
                                                        "p-3 bg-gray-50 border-l-4 border-gray-300 cursor-pointer hover:bg-gray-100"
                                                    }
                                                }
                                            >
                                                <p class="font-medium">{sched.name.clone()}</p>
                                                <p class="text-xs text-gray-600">
                                                    {format!("{} {} {} {} {}", sched.cron_minute, sched.cron_hour, sched.cron_day_of_month, sched.cron_month, sched.cron_day_of_week)}
                                                </p>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>

                            <div>
                                <h3 class="text-lg font-bold mb-2">Tasks</h3>
                                {move || {
                                    if selected_schedule.get().is_none() {
                                        return view! {
                                            <p class="text-gray-600">"Select a schedule to view its tasks"</p>
                                        }.into_any();
                                    }

                                    let tasks = schedule_tasks.get();
                                    if tasks.is_empty() {
                                        return view! {
                                            <p class="text-gray-600">"No tasks for this schedule"</p>
                                        }.into_any();
                                    }

                                    view! {
                                        <div class="space-y-2">
                                            {tasks.into_iter().map(|task| {
                                                view! {
                                                    <div class="p-3 bg-gray-50 rounded border border-gray-200">
                                                        <p class="font-medium">{task.action.clone()}</p>
                                                        <p class="text-xs text-gray-600">{task.payload.clone()}</p>
                                                        <p class="text-xs text-gray-400">
                                                            {"Offset: "}{task.time_offset}{" seconds"}
                                                        </p>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }}
                                <button
                                    on:click=move |_| add_task_modal.set(true)
                                    disabled=move || selected_schedule.get().is_none()
                                    class="mt-4 px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700 font-medium disabled:opacity-50"
                                >
                                    "Add Task"
                                </button>
                            </div>
                        </div>

                        {move || {
                            if selected_schedule.get().is_some() {
                                view! {
                                    <button
                                        on:click=move |_| delete_schedule_modal.set(true)
                                        class="mt-4 px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 font-medium"
                                    >
                                        "Delete Schedule"
                                    </button>
                                }.into_any()
                            } else {
                                view! { }.into_any()
                            }
                        }}
                    }.into_any()
                }}
            </Show>

            // Create schedule modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !create_schedule_modal.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| create_schedule_modal.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">Create Schedule</h3>
                        <button
                            on:click=move |_| create_schedule_modal.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium mb-1">Name</label>
                            <input
                                type="text"
                                placeholder="Schedule name"
                                value=move || sched_name.get()
                                on:input=move |e| sched_name.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div class="grid grid-cols-2 gap-2">
                            <div>
                                <label class="block text-xs font-medium mb-1">Minute</label>
                                <input type="text" value=move || sched_minute.get() on:input=move |e| sched_minute.set(event_target_value(&e)) class="w-full px-2 py-1 border border-gray-300 rounded text-sm" />
                            </div>
                            <div>
                                <label class="block text-xs font-medium mb-1">Hour</label>
                                <input type="text" value=move || sched_hour.get() on:input=move |e| sched_hour.set(event_target_value(&e)) class="w-full px-2 py-1 border border-gray-300 rounded text-sm" />
                            </div>
                            <div>
                                <label class="block text-xs font-medium mb-1">Day of Month</label>
                                <input type="text" value=move || sched_day_of_month.get() on:input=move |e| sched_day_of_month.set(event_target_value(&e)) class="w-full px-2 py-1 border border-gray-300 rounded text-sm" />
                            </div>
                            <div>
                                <label class="block text-xs font-medium mb-1">Month</label>
                                <input type="text" value=move || sched_month.get() on:input=move |e| sched_month.set(event_target_value(&e)) class="w-full px-2 py-1 border border-gray-300 rounded text-sm" />
                            </div>
                            <div class="col-span-2">
                                <label class="block text-xs font-medium mb-1">Day of Week</label>
                                <input type="text" value=move || sched_day_of_week.get() on:input=move |e| sched_day_of_week.set(event_target_value(&e)) class="w-full px-2 py-1 border border-gray-300 rounded text-sm" />
                            </div>
                        </div>
                        <div class="flex gap-2 justify-end pt-4">
                            <button
                                on:click=move |_| create_schedule_modal.set(false)
                                class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=move |_| on_create_schedule.run(())
                                class="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700"
                            >
                                "Create"
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            // Add task modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !add_task_modal.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| add_task_modal.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <div class="flex justify-between items-center mb-6">
                        <h3 class="text-xl font-bold">Add Task</h3>
                        <button
                            on:click=move |_| add_task_modal.set(false)
                            class="text-gray-400 hover:text-gray-600 text-2xl leading-none"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium mb-1">Action</label>
                            <select prop:value=move || task_action.get() on:input=move |e| task_action.set(event_target_value(&e)) class="w-full px-4 py-2 border border-gray-300 rounded">
                                <option value="command">"Command"</option>
                                <option value="power">"Power"</option>
                            </select>
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Payload</label>
                            <textarea
                                placeholder="Command or payload"
                                prop:value=move || task_payload.get()
                                on:input=move |e| task_payload.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                                rows="3"
                            />
                        </div>
                        <div>
                            <label class="block text-sm font-medium mb-1">Time Offset (seconds)</label>
                            <input
                                type="number"
                                placeholder="0"
                                value=move || task_time_offset.get()
                                on:input=move |e| task_time_offset.set(event_target_value(&e))
                                class="w-full px-4 py-2 border border-gray-300 rounded"
                            />
                        </div>
                        <div class="flex gap-2 justify-end pt-4">
                            <button
                                on:click=move |_| add_task_modal.set(false)
                                class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=move |_| on_add_task.run(())
                                class="px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700"
                            >
                                "Add"
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            // Delete confirm modal
            <div class="fixed inset-0 z-50 flex items-center justify-center" class=("hidden", move || !delete_schedule_modal.get())>
                <div class="absolute inset-0 bg-black bg-opacity-50" on:click=move |_| delete_schedule_modal.set(false) />
                <div class="relative bg-white rounded-lg shadow-xl p-8 w-full max-w-md mx-4">
                    <h3 class="text-xl font-bold mb-4">Delete Schedule?</h3>
                    <p class="text-gray-600 mb-6">This action cannot be undone.</p>
                    <div class="flex gap-2 justify-end">
                        <button
                            on:click=move |_| delete_schedule_modal.set(false)
                            class="px-4 py-2 text-gray-700 border border-gray-300 rounded hover:bg-gray-50"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=move |_| on_delete_schedule.run(())
                            class="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700"
                        >
                            "Delete"
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
