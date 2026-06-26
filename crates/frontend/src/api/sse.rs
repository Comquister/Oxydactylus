use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::EventSource;

/// Acumula todas as linhas recebidas. Fecha o EventSource ao desmontar o componente.
/// DEVE ser chamado no nível do componente (dentro do escopo reativo), nunca dentro
/// de callbacks — on_cleanup perderia o Reactive Owner e a conexão não seria fechada.
pub fn use_sse(url: String) -> RwSignal<Vec<String>> {
    let lines = RwSignal::new(Vec::<String>::new());
    let source = EventSource::new(&url).expect("EventSource");

    let onmessage = Closure::wrap(Box::new({
        let lines = lines.clone();
        move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            lines.update(|v| v.push(data));
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let source_close = source.clone();
    // `let _ = onmessage` move a Closure para dentro do on_cleanup, mantendo-a viva
    // até o desmonte e liberando heap WASM quando o componente for destruído.
    // NÃO usar onmessage.forget() — alocaria permanentemente no heap WASM.
    on_cleanup(move || {
        source_close.close();
        let _ = onmessage;
    });

    lines
}

/// Versão baseada em callback — use para stats/valores que se substituem a cada tick.
/// Evita crescimento linear de memória (ao contrário de acumular em Vec).
/// DEVE ser chamado no nível do componente pelo mesmo motivo de `use_sse`.
pub fn use_sse_callback(url: String, mut on_message: impl FnMut(String) + 'static) {
    let source = EventSource::new(&url).expect("EventSource");

    let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let data = event.data().as_string().unwrap_or_default();
        on_message(data);
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let source_close = source.clone();
    on_cleanup(move || {
        source_close.close();
        let _ = onmessage;
    });
}
