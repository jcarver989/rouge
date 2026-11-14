use js_sys::{Function, Object, Reflect};
use tracing::Subscriber;
use tracing::span::{Attributes, Id};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;
use wasm_bindgen::{JsCast, JsValue};

struct Started(f64);

pub(crate) struct PerformanceLayer;

impl<T: Subscriber + for<'a> LookupSpan<'a>> Layer<T> for PerformanceLayer {
    fn on_new_span(&self, _: &Attributes<'_>, _: &Id, _: Context<'_, T>) {}

    fn on_enter(&self, id: &Id, cx: Context<'_, T>) {
        if let (Some(span), Some(now)) = (cx.span(id), now()) {
            span.extensions_mut().replace(Started(now));
        }
    }

    fn on_exit(&self, id: &Id, cx: Context<'_, T>) {
        let Some(span) = cx.span(id) else {
            return;
        };
        let Some(Started(start)) = span.extensions_mut().remove::<Started>() else {
            return;
        };
        let _ = measure(&format!("rouge:{}", span.name()), start);
    }
}

fn now() -> Option<f64> {
    Some(web_sys::window()?.performance()?.now())
}

fn measure(name: &str, start: f64) -> Result<(), JsValue> {
    let performance = web_sys::window()
        .and_then(|window| window.performance())
        .ok_or_else(|| JsValue::from_str("no performance timeline"))?;
    let options = Object::new();
    Reflect::set(&options, &"start".into(), &start.into())?;
    Reflect::set(&options, &"end".into(), &performance.now().into())?;
    let measure: Function = Reflect::get(&performance, &"measure".into())?.dyn_into()?;
    measure.call2(&performance, &name.into(), &options)?;
    Ok(())
}
