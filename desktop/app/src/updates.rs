//! Looking for a newer version, and the one quiet icon that offers it.
//!
//! One check shortly after start and another every few hours, only when the setting is on.
//! Nothing is downloaded or installed until the viewer presses the icon (or "Update" in
//! Settings), and nothing here ever covers the watch page.

use crate::backend::Backend;
use crate::design::button::icon_button;
use crate::design::tokens::{ACCENT, CANVAS, DANGER, LINE, RAISED, TEXT, color};
use crate::runtime;
use crate::updater::{self, Installed};
use akidev_core::update::Update;
use gpui_kit::assets::IconName;
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

/// How far an update has got, written from the network thread and read by the icon.
#[derive(Default)]
struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    stage: AtomicU8,
}

impl Progress {
    fn snapshot(&self) -> (u64, u64, u8) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            self.stage.load(Ordering::Relaxed),
        )
    }
}

enum Phase {
    Ready,
    Working,
    /// Installed; the app is about to start the new version.
    Restarting,
    /// It did not install, in one short sentence; pressing again tries again.
    Failed(String),
    /// It cannot be installed from here (the reason is logged); the download page is opened.
    Page,
}

pub struct Updates {
    update: Option<Update>,
    phase: Phase,
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
            progress: Arc::default(),
        }
    }

    fn found(&mut self, found: Option<Update>, cx: &mut Context<Self>) {
        // Never swap what is on offer while it is being installed.
        if matches!(self.phase, Phase::Working | Phase::Restarting) {
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

    /// Whether the icon is drawn: an update is known and checking is on.
    fn offered(&self, cx: &App) -> bool {
        self.update.is_some() && cx.global::<Backend>().config.check_for_updates
    }

    /// What pressing the icon (or Settings' "Update") does in the current state.
    pub fn activate(&mut self, cx: &mut Context<Self>) {
        match self.phase {
            Phase::Ready | Phase::Failed(_) => self.run(cx),
            Phase::Page => cx.open_url(&updater::download_page(self.update.as_ref())),
            Phase::Working | Phase::Restarting => {}
        }
    }

    /// Downloads, checks and installs, then restarts. Only ever started by a press.
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
                Ok(Installed::OpenPage(reason)) => {
                    tracing::info!("[update] not installed here: {reason}");
                    updates.phase = Phase::Page;
                    cx.notify();
                    updates.activate(cx);
                }
                Err(reason) => {
                    tracing::warn!("[update] failed: {reason}");
                    crate::analytics::error("update", &reason);
                    updates.phase = Phase::Failed(failure_sentence(&reason));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Restarting;
        cx.notify();
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

    /// The one icon in the top bar, or nothing. A dot says an update is waiting; a thin ring
    /// shows it being installed; a red dot says it did not work. Click does the obvious thing.
    pub fn button(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        self.update.as_ref().filter(|_| self.offered(cx))?;
        let updates = cx.entity();
        let button = icon_button(
            "update",
            IconName::ArrowDownToLine,
            cx.listener(|updates, _, _, cx| updates.activate(cx)),
        )
        .relative()
        .tooltip(move |_, cx| {
            let updates = updates.clone();
            cx.new(|cx| Tip::new(updates, cx)).into()
        });
        Some(match &self.phase {
            Phase::Ready | Phase::Page => button.child(dot(ACCENT)),
            Phase::Failed(_) => button.child(dot(DANGER)),
            Phase::Working => {
                let (done, total, stage) = self.progress.snapshot();
                match download_fraction(done, total, stage) {
                    Some(fraction) => button.child(ring(-90., fraction.max(0.02) * 360.)),
                    None => button.child(spinner()),
                }
            }
            Phase::Restarting => button.child(ring(-90., 360.)),
        })
    }

    /// What the tooltip and the settings row say about the update, in a few words.
    fn label(&self) -> String {
        let version = self.update.as_ref().map_or("", |update| &update.version);
        match &self.phase {
            Phase::Ready => format!("Update to {version}"),
            Phase::Working => {
                let (done, total, stage) = self.progress.snapshot();
                progress_label(done, total, stage)
            }
            Phase::Restarting => "Restarting".into(),
            Phase::Failed(sentence) => sentence.clone(),
            Phase::Page => "Open the download page".into(),
        }
    }

    /// What Settings shows beside the version.
    pub fn status(&self, cx: &App) -> Status {
        let Some(update) = self.update.as_ref().filter(|_| self.offered(cx)) else {
            return Status::UpToDate;
        };
        match &self.phase {
            Phase::Ready => Status::Available(update.version.clone()),
            Phase::Working | Phase::Restarting => Status::Busy(self.label()),
            Phase::Failed(sentence) => Status::Failed(sentence.clone()),
            Phase::Page => Status::Page,
        }
    }
}

/// What Settings shows about updates.
pub enum Status {
    UpToDate,
    Available(String),
    Busy(String),
    Failed(String),
    Page,
}

/// The update state, for the screens that are not the top bar.
pub struct Shared(pub Entity<Updates>);

impl Global for Shared {}

/// The tooltip over the icon. It reads the state each time, so "Downloading 43%" keeps moving
/// while the pointer rests on it.
struct Tip {
    updates: Entity<Updates>,
    _watch: Subscription,
}

impl Tip {
    fn new(updates: Entity<Updates>, cx: &mut Context<Self>) -> Tip {
        let watch = cx.observe(&updates, |_, _, cx| cx.notify());
        Tip {
            updates,
            _watch: watch,
        }
    }
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .mt(px(10.))
            .ml(px(6.))
            .px_2()
            .py_1()
            .rounded_md()
            .bg(color(RAISED))
            .border_1()
            .border_color(color(LINE))
            .shadow_md()
            .text_size(px(12.))
            .text_color(color(TEXT))
            .child(self.updates.read(cx).label())
    }
}

const RING: f32 = 28.;
const RING_RADIUS: f32 = 12.;

/// The marker at the icon's top right: a 6px dot inside a 1.5px ring of the bar's colour, so
/// it reads on any artwork. It grows and fades in over a moment when it first appears.
fn dot(fill: u32) -> impl IntoElement {
    div()
        .absolute()
        .top(px(6.))
        .left(px(22.))
        .size(px(9.))
        .flex()
        .items_center()
        .justify_center()
        .with_animation(
            "update-dot",
            Animation::new(Duration::from_millis(180)),
            move |holder, t| {
                let t = 1. - (1. - t).powi(3);
                holder.opacity(t).child(
                    div()
                        .size(px(9. * t))
                        .p(px(1.5 * t))
                        .rounded_full()
                        .bg(color(CANVAS))
                        .child(div().size_full().rounded_full().bg(color(fill))),
                )
            },
        )
}

/// A thin circular progress ring around the icon: a faint track and an accent arc from
/// `start` degrees (0 is three o'clock) sweeping `sweep` degrees clockwise.
fn ring(start: f32, sweep: f32) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let middle = bounds.center();
            let point = |degrees: f32| {
                let radians = degrees.to_radians();
                gpui_kit::point(
                    middle.x + px(RING_RADIUS * radians.cos()),
                    middle.y + px(RING_RADIUS * radians.sin()),
                )
            };
            let stroke = |points: &[Point<Pixels>], closed: bool| {
                let mut path = PathBuilder::stroke(px(2.));
                path.add_polygon(points, closed);
                path.build().ok()
            };
            let track: Vec<_> = (0..72).map(|step| point(step as f32 * 5.)).collect();
            if let Some(path) = stroke(&track, true) {
                window.paint_path(path, color(TEXT).opacity(0.14));
            }
            if sweep >= 359. {
                if let Some(path) = stroke(&track, true) {
                    window.paint_path(path, color(ACCENT));
                }
                return;
            }
            let steps = (sweep / 4.).ceil().max(1.) as usize;
            let arc: Vec<_> = (0..=steps)
                .map(|step| point(start + sweep * step as f32 / steps as f32))
                .collect();
            if let Some(path) = stroke(&arc, false) {
                window.paint_path(path, color(ACCENT));
            }
            // Round caps: a small disc at each end of the arc.
            for end in [start, start + sweep] {
                let at = point(end);
                let disc: Vec<_> = (0..16)
                    .map(|step| {
                        let radians = (step as f32 * 22.5).to_radians();
                        gpui_kit::point(at.x + px(radians.cos()), at.y + px(radians.sin()))
                    })
                    .collect();
                let mut path = PathBuilder::fill();
                path.add_polygon(&disc, true);
                if let Ok(path) = path.build() {
                    window.paint_path(path, color(ACCENT));
                }
            }
        },
    )
    .absolute()
    .top(px((36. - RING) / 2.))
    .left(px((36. - RING) / 2.))
    .size(px(RING))
}

/// The ring with a quarter arc going slowly round, for when there is no percentage to show.
fn spinner() -> impl IntoElement {
    ring(-90., 100.).with_animation(
        "update-spin",
        Animation::new(Duration::from_millis(1600)).repeat(),
        |_, t| ring(-90. + 360. * t, 100.),
    )
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

/// Why an update failed, as one short sentence for a tooltip.
fn failure_sentence(reason: &str) -> String {
    let reason = reason.to_lowercase();
    let because = if reason.contains("hash") {
        "the download didn't match its checksum"
    } else if reason.contains("signature") || reason.contains("apple team") {
        "the new version's signature didn't check out"
    } else if reason.contains("too large") || reason.contains("cannot be used") {
        "the release can't be installed"
    } else if reason.contains("disk image")
        || reason.contains("copying")
        || reason.contains("app in place")
        || reason.contains("moving")
        || reason.contains("restoring")
        || reason.contains("restarting")
    {
        "the install didn't finish"
    } else {
        "the download didn't finish"
    };
    format!("Couldn't update: {because}")
}

/// How far the download is, or `None` when there is nothing to show a fraction of (the size
/// is unknown, or it is past downloading and into checking and installing).
fn download_fraction(done: u64, total: u64, stage: u8) -> Option<f32> {
    (stage != INSTALLING && total > 0 && done < total).then(|| done as f32 / total as f32)
}

/// "Downloading 43%", then "Installing" once the file is whole and being checked.
fn progress_label(done: u64, total: u64, stage: u8) -> String {
    match download_fraction(done, total, stage) {
        Some(fraction) => format!("Downloading {}%", (fraction * 100.) as u32),
        None if stage == INSTALLING || (total > 0 && done >= total) => "Installing".into(),
        None => "Downloading".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{INSTALLING, failure_sentence, progress_label};

    #[test]
    fn a_failure_is_one_short_sentence() {
        assert_eq!(
            failure_sentence("the download does not match its published hash"),
            "Couldn't update: the download didn't match its checksum"
        );
        assert_eq!(
            failure_sentence("the app's signature is not valid"),
            "Couldn't update: the new version's signature didn't check out"
        );
        assert_eq!(
            failure_sentence("error sending request for url"),
            "Couldn't update: the download didn't finish"
        );
    }

    #[test]
    fn the_stage_is_named_in_a_word_or_two() {
        assert_eq!(progress_label(0, 0, 0), "Downloading");
        assert_eq!(progress_label(43, 100, 0), "Downloading 43%");
        assert_eq!(progress_label(100, 100, 0), "Installing");
        assert_eq!(progress_label(10, 100, INSTALLING), "Installing");
    }
}
