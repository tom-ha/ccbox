//! `ComponentContext` — borrowed inputs every component reads.

use crate::config::Env;
use crate::input::session::SessionInfo;
use crate::render::Renderer;

use super::render_cache::RenderCache;

/// Read-only view passed to every `Component::is_visible` and
/// `Component::render` call. Lives for the duration of a single
/// `render(...)` invocation.
pub struct ComponentContext<'a> {
    pub session: &'a SessionInfo,
    pub env: &'a Env,
    pub renderer: &'a Renderer,
    pub width: i32,
    pub now: f64,
    pub data: &'a RenderCache,
}

impl<'a> ComponentContext<'a> {
    pub fn new(
        session: &'a SessionInfo,
        env: &'a Env,
        renderer: &'a Renderer,
        width: i32,
        now: f64,
        data: &'a RenderCache,
    ) -> Self {
        Self {
            session,
            env,
            renderer,
            width,
            now,
            data,
        }
    }
}
