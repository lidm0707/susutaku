use crate::renderer::{Renderer, RendererConfig};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::prelude::{Closure, wasm_bindgen};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;

const DEFAULT_SPEED: f32 = 1.0;
const MS_TO_SECONDS: f32 = 1.0 / 1000.0;

#[wasm_bindgen]
pub async fn start(canvas: HtmlCanvasElement) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    let mut renderer = Renderer::new(RendererConfig {
        surface_target: wgpu::SurfaceTarget::Canvas(canvas),
        width,
        height,
        speed: DEFAULT_SPEED,
    })
    .await
    .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let last = Rc::new(Cell::new(now_ms(&window)));
    loop {
        wait_frame(&window).await?;
        let now = now_ms(&window);
        let dt = ((now - last.get()) * f64::from(MS_TO_SECONDS)) as f32;
        last.set(now);
        renderer
            .render_frame(dt)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
    }
}

fn now_ms(window: &web_sys::Window) -> f64 {
    window.performance().map(|p| p.now()).unwrap_or_default()
}

async fn wait_frame(window: &web_sys::Window) -> Result<(), JsValue> {
    let promise = js_sys::Promise::new(
        &mut |resolve: js_sys::Function, _reject: js_sys::Function| {
            let resolve = wasm_bindgen::JsValue::from(resolve);
            let on_frame = Closure::wrap(Box::new(move |_ms: f64| {
                let resolve: js_sys::Function = resolve.clone().unchecked_into();
                resolve.call0(&JsValue::NULL).ok();
            }) as Box<dyn FnMut(f64)>);
            let callback: js_sys::Function = on_frame.into_js_value().unchecked_into();
            window.request_animation_frame(&callback).ok();
        },
    );
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    Ok(())
}
