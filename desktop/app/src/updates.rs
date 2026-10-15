//! Looking for a newer version, and the quiet pill and card that offer it.
//!
//! One check shortly after start and another every few hours, only when the setting is on.
//! Nothing is downloaded or installed until the viewer presses "Update and restart", and
//! nothing here ever covers the watch page.

use crate::backend::Backend;
use crate::design::button::{primary_button, secondary_button};
use crate::design::tokens::{ACCENT, LINE, SURFACE, TEXT, TEXT_DIM, TEXT_FAINT, color, size};
use crate::runtime;
use crate::updater::{self, Installed};
use akidev_core::update::Update;
use gpui_kit::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

/// After start, so a check never competes with the first screen or with playback.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// A Windows installer that closes the app itself gets this long to do it before we quit.
const INSTALLER_GRACE: Duration = Duration::from_secs(3);

const INSTALLING: u8 = 1;

/// How far an update has got, written from the network thread and read by the card.
#[derive(Default)]
struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    stage: AtomicU8,
}

enum Phase {
    Ready,
    Working,
    /// It did not install, and why; the download page is offered instead.
    Failed(String),
}

pub struct Updates {
    update: Option<Update>,
    phase: Phase,
    /// "Later": hidden until the next start.
    hidden: bool,
    open: bool,
    progress: Arc<Progress>,
}

impl Updates {
    pub fn new(cx: &mut Context<Self>) -> Updates {
        cx.spawn(async move |updates, cx| {
            cx.background_executor().timer(FIRST_CHECK).await;
            loop {
                let Ok(enabled) =
                    updates.update(cx, |_, cx| cx.global::<Backend>().config.check_for_updates)
                else {
                    break;
                };
                if enabled {
                    let current = updater::current_version();
                    let found =
                        runtime::run(async move { akidev_core::update::check(&current).await })
                            .await;
                    match found {
                        Some(Ok(found)) => {
                            let _ = updates.update(cx, |updates, cx| updates.found(found, cx));
                        }
                        Some(Err(error)) => tracing::info!("[update] check failed: {error:#}"),
                        None => {}
                    }
                }
                cx.background_executor().timer(EVERY).await;
            }
        })
        .detach();
        Updates {
            update: None,
            phase: Phase::Ready,
            hidden: false,
            open: false,
            progress: Arc::default(),
        }
    }

    fn found(&mut self, found: Option<Update>, cx: &mut Context<Self>) {
        // Never swap what is on offer while it is being installed.
        if matches!(self.phase, Phase::Working) {
            return;
        }
        if let Some(found) = found
            && self.update.as_ref().map(|update| &update.version) != Some(&found.version)
        {
            tracing::info!("[update] {} is available", found.version);
            self.update = Some(found);
            self.phase = Phase::Ready;
            cx.notify();
        }
    }

    fn working(&self) -> bool {
        matches!(self.phase, Phase::Working)
    }

    /// Whether the pill is drawn: an update is known, and the viewer has not said "Later".
    fn offered(&self, cx: &App) -> bool {
        self.update.is_some() && !self.hidden && cx.global::<Backend>().config.check_for_updates
    }

    fn later(&mut self, cx: &mut Context<Self>) {
        self.hidden = true;
        self.open = false;
        cx.notify();
    }

    fn open_page(&mut self, cx: &mut Context<Self>) {
        cx.open_url(&updater::download_page(self.update.as_ref()));
    }

    /// Downloads, checks and installs, then restarts. Only ever started by a button.
    fn run(&mut self, cx: &mut Context<Self>) {
        let Some(update) = self.update.clone() else {
            return;
        };
        if self.working() {
            return;
        }
        self.phase = Phase::Working;
        let progress = Arc::new(Progress::default());
        self.progress = progress.clone();
        cx.notify();

        // The percentage is read a few times a second rather than pushed on every chunk.
        cx.spawn(async move |updates, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if !updates
                    .update(cx, |updates, cx| {
                        cx.notify();
                        updates.working()
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |updates, cx| {
            let outcome = runtime::run(apply(update, progress)).await;
            let outcome = outcome.unwrap_or_else(|| Err("The update stopped unexpectedly.".into()));
            let _ = updates.update(cx, |updates, cx| match outcome {
                Ok(Installed::Restart) => updates.restart(cx),
                Ok(Installed::OpenPage(reason)) | Err(reason) => {
                    updates.phase = Phase::Failed(reason);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        if cfg!(windows) {
            // The installer closes this app and starts the new one; quitting is the fallback.
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(INSTALLER_GRACE).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        } else {
            cx.quit();
        }
    }

    /// The pill in the top bar, or nothing.
    pub fn pill(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let update = self.update.as_ref().filter(|_| self.offered(cx))?;
        Some(
            div()
                .id("update-pill")
                .flex()
                .items_center()
                .gap_1p5()
                .h(px(28.))
                .px_3()
                .rounded_full()
                .bg(color(ACCENT).opacity(0.12))
                .hover(|style| style.bg(color(ACCENT).opacity(0.2)))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(ACCENT))
                .on_click(cx.listener(|updates, _, _, cx| {
                    updates.open = !updates.open;
                    cx.notify();
                }))
                .child(if self.working() {
                    "Updating…"
                } else {
                    "Update available"
                })
                .child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(color(TEXT_FAINT))
                        .child(update.version.clone()),
                ),
        )
    }

    /// The card under the pill, when it is open.
    pub fn card(&self, cx: &mut Context<Self>) -> Option<Div> {
        let update = self
            .update
            .as_ref()
            .filter(|_| self.offered(cx) && self.open)?;
        let heading = |text: String| {
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::BOLD)
                .text_color(color(TEXT))
                .child(text)
        };
        let quiet = |text: String| {
            div()
                .text_size(px(size::BODY))
                .text_color(color(TEXT_DIM))
                .child(text)
        };

        let body = match &self.phase {
            Phase::Ready => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(heading(format!("Akidev {} is available", update.version)))
                .children(
                    (!update.notes.is_empty())
                        .then(|| quiet(release_notes(&update.notes)).line_clamp(4)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .pt_1()
                        .child(primary_button(
                            "update-now",
                            "Update and restart",
                            None,
                            cx.listener(|updates, _, _, cx| updates.run(cx)),
                        ))
                        .child(secondary_button(
                            "update-later",
                            "Later",
                            None,
                            cx.listener(|updates, _, _, cx| updates.later(cx)),
                        )),
                ),
            Phase::Working => {
                let (done, total, stage) = (
                    self.progress.done.load(Ordering::Relaxed),
                    self.progress.total.load(Ordering::Relaxed),
                    self.progress.stage.load(Ordering::Relaxed),
                );
                let fraction = if total == 0 {
                    0.
                } else {
                    (done as f32 / total as f32).min(1.)
                };
                let label = match stage {
                    INSTALLING => "Installing…".to_string(),
                    _ if total > 0 && done >= total => "Verifying…".to_string(),
                    _ => format!("Downloading {}%", (fraction * 100.) as u32),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(heading(format!("Updating to Akidev {}", update.version)))
                    .child(quiet(label))
                    .child(
                        div()
                            .h(px(3.))
                            .w_full()
                            .rounded_full()
                            .bg(color(TEXT).opacity(0.14))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(if stage == INSTALLING { 1. } else { fraction }))
                                    .rounded_full()
                                    .bg(color(ACCENT)),
                            ),
                    )
            }
            Phase::Failed(reason) => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(heading("Akidev could not update itself".into()))
                .child(quiet(reason.clone()))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .pt_1()
                        .child(primary_button(
                            "update-page",
                            "Open download page",
                            None,
                            cx.listener(|updates, _, _, cx| updates.open_page(cx)),
                        ))
                        .child(secondary_button(
                            "update-close",
                            "Later",
                            None,
                            cx.listener(|updates, _, _, cx| updates.later(cx)),
                        )),
                ),
        };

        Some(
            div()
                .w(px(340.))
                .p_4()
                .rounded_xl()
                .bg(color(SURFACE))
                .border_1()
                .border_color(color(LINE))
                .shadow_lg()
                .occlude()
                .child(body),
        )
    }
}

/// Download, check and install, reporting how far it got.
async fn apply(update: Update, progress: Arc<Progress>) -> Result<Installed, String> {
    if let Err(reason) = updater::preflight() {
        return Ok(Installed::OpenPage(reason));
    }
    let dir = download_dir();
    let outcome = async {
        let path = akidev_core::update::download(&update, &dir, |done, total| {
            progress.done.store(done, Ordering::Relaxed);
            progress.total.store(total, Ordering::Relaxed);
        })
        .await
        .map_err(|error| format!("{error:#}"))?;
        progress.stage.store(INSTALLING, Ordering::Relaxed);
        tokio::task::spawn_blocking(move || updater::install(&path, &update))
            .await
            .map_err(|error| format!("{error}"))?
            .map_err(|error| format!("{error:#}"))
    }
    .await;
    // The installer a Windows restart is running from stays; everything else is removed.
    if !matches!(outcome, Ok(Installed::Restart)) {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome
}

fn download_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("live", "akidev", "Akidev")
        .map(|dirs| dirs.cache_dir().join("update"))
        .unwrap_or_else(|| std::env::temp_dir().join("akidev-update"))
}

/// The first lines of a release's notes, without the markdown marks.
fn release_notes(notes: &str) -> String {
    notes
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '*', '-', ' ']).trim())
        .filter(|line| !line.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::release_notes;

    #[test]
    fn notes_lose_their_markdown_marks_and_blank_lines() {
        assert_eq!(
            release_notes("## What's new\n\n- Faster start\n* Fixes\n\n\n- a\n- b\n- c"),
            "What's new\nFaster start\nFixes\na"
        );
        assert_eq!(release_notes("  \n"), "");
    }
}
