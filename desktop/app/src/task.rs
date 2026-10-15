//! Background work for views.

use crate::runtime;
use gpui_kit::Context;
use std::future::Future;

/// Runs `work` on the network runtime, then hands its result to `done` on the view.
/// If the view was closed in the meantime, or the work failed outright, nothing happens.
pub fn load<V: 'static, T: Send + 'static>(
    cx: &mut Context<V>,
    work: impl Future<Output = T> + Send + 'static,
    done: impl FnOnce(&mut V, T, &mut Context<V>) + 'static,
) {
    cx.spawn(async move |view, cx| {
        let Some(result) = runtime::run(work).await else {
            return;
        };
        let _ = view.update(cx, |view, cx| done(view, result, cx));
    })
    .detach();
}
