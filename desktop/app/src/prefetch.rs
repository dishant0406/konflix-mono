//! Asks for a title's details as soon as the pointer rests on its tile, so they are usually
//! there by the time it is clicked.

use akidev_core::providers;
use gpui_kit::App;
use std::sync::Mutex;
use std::time::Duration;

/// How long the pointer must stay on a tile before it counts as interest and not as passing by.
const RESTS_FOR: Duration = Duration::from_millis(200);

/// The tile the pointer is on.
static HOVERED: Mutex<Option<String>> = Mutex::new(None);

/// The pointer moved onto, or off, the tile of the title `id`.
pub fn hovered(id: &str, on: bool, cx: &mut App) {
    let mut current = HOVERED.lock().unwrap();
    if !on {
        if current.as_deref() == Some(id) {
            *current = None;
        }
        return;
    }
    *current = Some(id.to_string());
    drop(current);
    let id = id.to_string();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(RESTS_FOR).await;
        if HOVERED.lock().unwrap().as_deref() == Some(&id) {
            crate::runtime::spawn(async move {
                let _ = providers::detail(&id).await;
            });
        }
    })
    .detach();
}
