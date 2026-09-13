mod anim;
mod renderer;

pub use anim::Animation;
pub use renderer::Renderer;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::start;
#[cfg(target_arch = "wasm32")]
mod capscreen;
#[cfg(target_arch = "wasm32")]
pub use capscreen::ScreenAnnotator;
