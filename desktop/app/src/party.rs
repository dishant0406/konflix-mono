//! Watch parties in the app: the party this window is in, the panel that starts and joins one,
//! and what the video and music players need from it.
//!
//! The host is this device or another one; guests connect to it directly (see `party/PROTOCOL.md`).
//! A player that is the host's tells the party what is playing with `publish`; a player that is a
//! guest's reads `target` and follows it, and asks the host for what a person would do.

use crate::design::button::icon_button;
use crate::design::tokens::{
    ACCENT, CANVAS, DANGER, LINE, SURFACE, TEXT, TEXT_DIM, TEXT_FAINT, TOP_BAR, color,
};
use crate::route::{self, Route};
use crate::runtime;
use akidev_party::{
    Guest, GuestEvent, Host, HostEvent, Item, Member, Message, Playback, Readiness, Target,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

/// Where the meeting point is. `AKIDEV_API` points a development build at another server.
const API: &str = "https://api.akidev.live";
const SITE: &str = "https://akidev.live";

fn api() -> String {
    std::env::var("AKIDEV_API").unwrap_or_else(|_| API.to_string())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    None,
    Host,
    Guest,
}

/// Told to the players of a host when a guest asks for something.
pub enum PartyEvent {
    Play,
    Pause,
    Seek(f64),
}

impl EventEmitter<PartyEvent> for Party {}

pub struct PartyHandle(pub Entity<Party>);

impl Global for PartyHandle {}

pub fn party(cx: &App) -> Entity<Party> {
    cx.global::<PartyHandle>().0.clone()
}

/// Opens the panel, or closes it.
pub fn toggle_panel(cx: &mut App) {
    party(cx).update(cx, |party, cx| {
        party.panel_open = !party.panel_open;
        cx.notify();
    });
}

/// The party button for a bar: an accent dot on it while this window is in a party.
pub fn button(cx: &mut App) -> Stateful<Div> {
    let live = party(cx).read(cx).role() != Role::None;
    icon_button("party", IconName::Users, |_, _, cx| toggle_panel(cx))
        .relative()
        .when(live, |button| {
            button.child(
                div()
                    .absolute()
                    .top(px(6.))
                    .right(px(6.))
                    .size(px(8.))
                    .rounded_full()
                    .bg(color(ACCENT)),
            )
        })
}

/// The panel's two pages.
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Start,
    Join,
}

#[derive(Clone, Copy, PartialEq)]
enum Copied {
    Code,
    Link,
}

/// The letters and digits a code is made of, as the server makes it: the ones that are not easily
/// mistaken for each other.
const CODE_LENGTH: usize = 6;

fn clean_code(text: &str) -> String {
    text.to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase() && !matches!(c, 'I' | 'O') || matches!(c, '2'..='9'))
        .take(CODE_LENGTH)
        .collect()
}

pub struct Party {
    role: Role,
    connecting: bool,
    code: Option<String>,
    members: Vec<Member>,
    /// Why the last attempt to start or join failed, or the party stopped working.
    error: Option<String>,
    /// What the party is playing: what the host last published, or what the host last said.
    item: Option<Item>,
    /// A guest's latest word from the host.
    target: Option<Target>,
    panel_open: bool,
    /// Which of the panel's two pages is showing while this window is not in a party.
    tab: Tab,
    /// What was last copied, to say so for a moment.
    copied: Option<Copied>,
    name: Entity<InputState>,
    join_code: Entity<InputState>,
    _join_code_changes: Subscription,
    host: Option<Arc<Host>>,
    guest: Option<Arc<Guest>>,
    /// The host's updates are sent in order, by one task.
    publishing: Option<UnboundedSender<Playback>>,
    /// The item last published, so a heartbeat from something that is not playing is ignored.
    published: Option<Item>,
    /// What a guest last told the host, so it only tells it when that changes.
    said: Option<(Readiness, Option<String>)>,
    /// Counts the parties this window has been in, so a late event from an earlier one is ignored.
    session: u64,
}

impl Party {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Party {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Your name"));
        let own_name = format!("Desktop {}", 1000 + std::process::id() % 9000);
        name.update(cx, |input, cx| input.set_value(own_name, window, cx));
        let join_code = cx.new(|cx| InputState::new(window, cx));
        // The code is kept to its letters, and the last one joins, so there is nothing more to press.
        let join_code_changes = cx.subscribe_in(
            &join_code,
            window,
            |party, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let typed = input.read(cx).value().to_string();
                let cleaned = clean_code(&typed);
                if cleaned != typed {
                    input.update(cx, |input, cx| input.set_value(cleaned, window, cx));
                    return;
                }
                cx.notify();
                if cleaned.chars().count() == CODE_LENGTH
                    && !party.connecting
                    && party.role == Role::None
                {
                    party.join(cx);
                }
            },
        );
        Party {
            role: Role::None,
            connecting: false,
            code: None,
            members: Vec::new(),
            error: None,
            item: None,
            target: None,
            panel_open: false,
            tab: Tab::Start,
            copied: None,
            name,
            join_code,
            _join_code_changes: join_code_changes,
            host: None,
            guest: None,
            publishing: None,
            published: None,
            said: None,
            session: 0,
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// A guest's latest word from the host: what to play and where.
    pub fn target(&self) -> Option<&Target> {
        self.target.as_ref()
    }

    /// What the party is playing.
    pub fn item(&self) -> Option<&Item> {
        self.item.as_ref()
    }

    pub fn is_open(&self) -> bool {
        self.panel_open
    }

    /// The host says what is playing: when it changes, and every couple of seconds while it plays.
    /// A `heartbeat` from something that is not what is playing is ignored.
    pub fn publish(&mut self, playback: Playback, heartbeat: bool) {
        let Some(publishing) = &self.publishing else {
            return;
        };
        if heartbeat && playback.item != self.published {
            return;
        }
        self.published = playback.item.clone();
        self.item = playback.item.clone();
        let _ = publishing.send(playback);
    }

    /// A guest asks the host to play, pause or seek.
    pub fn ask(&self, message: Message) {
        if let Some(guest) = self.guest.clone() {
            runtime::spawn(async move { guest.ask(message).await });
        }
    }

    /// A guest says how it is doing with the item whose id is `id`: ready for it, still loading it
    /// (a film from a torrent can take a while), or unable to play it.
    pub fn set_status(&mut self, state: Readiness, id: Option<String>) {
        let Some(guest) = self.guest.clone() else {
            return;
        };
        if self
            .said
            .as_ref()
            .is_some_and(|said| *said == (state, id.clone()))
        {
            return;
        }
        self.said = Some((state, id.clone()));
        runtime::spawn(async move { guest.status(state, id).await });
    }

    /// The names of the guests who are still loading what the host is playing, for the host.
    pub fn waiting_for(&self) -> Vec<String> {
        self.members
            .iter()
            .filter(|member| !member.host && member.state == Readiness::Loading)
            .map(|member| member.name.clone())
            .collect()
    }

    /// Whether anyone else is in the party.
    pub fn has_company(&self) -> bool {
        self.members.len() > 1
    }

    fn name(&self, cx: &App) -> String {
        self.name.read(cx).value().trim().to_string()
    }

    fn reset(&mut self) {
        self.session += 1;
        self.role = Role::None;
        self.connecting = false;
        self.code = None;
        self.members.clear();
        self.item = None;
        self.target = None;
        self.host = None;
        self.guest = None;
        self.publishing = None;
        self.published = None;
        self.said = None;
    }

    /// Ends or leaves the party, whichever this is.
    pub fn leave(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = self.host.clone() {
            runtime::spawn(async move { host.end().await });
        }
        if let Some(guest) = self.guest.clone() {
            runtime::spawn(async move { guest.leave().await });
        }
        self.reset();
        cx.notify();
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        self.leave(cx);
        self.error = None;
        self.connecting = true;
        let session = self.session;
        let name = match self.name(cx) {
            name if name.is_empty() => "Host".to_string(),
            name => name,
        };
        cx.notify();
        cx.spawn(async move |party, cx| {
            let made = runtime::run(async move { Host::create(&api(), &name).await }).await;
            let _ = party.update(cx, |party, cx| {
                if party.session != session {
                    return;
                }
                party.connecting = false;
                match made {
                    Some(Ok((host, events))) => party.hosting(host, events, cx),
                    Some(Err(error)) => party.error = Some(format!("{error:#}")),
                    None => party.error = Some("Could not start the party.".into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn hosting(
        &mut self,
        host: Host,
        mut events: tokio::sync::mpsc::UnboundedReceiver<HostEvent>,
        cx: &mut Context<Self>,
    ) {
        let host = Arc::new(host);
        let (publishing, mut updates) = tokio::sync::mpsc::unbounded_channel::<Playback>();
        let sender = host.clone();
        runtime::spawn(async move {
            while let Some(playback) = updates.recv().await {
                sender.publish(playback).await;
            }
        });
        self.role = Role::Host;
        self.code = Some(host.code.clone());
        self.members = vec![Member {
            name: self.name(cx),
            host: true,
            state: Readiness::Ready,
        }];
        self.host = Some(host);
        self.publishing = Some(publishing);
        let session = self.session;
        cx.spawn(async move |party, cx| {
            while let Some(event) = events.recv().await {
                let alive = party.update(cx, |party, cx| {
                    if party.session != session {
                        return false;
                    }
                    match event {
                        HostEvent::Members(members) => party.members = members,
                        HostEvent::Unreachable => {
                            party.error = Some(
                                "Lost contact with the party server, so no one new can join."
                                    .into(),
                            );
                        }
                        HostEvent::Play => cx.emit(PartyEvent::Play),
                        HostEvent::Pause => cx.emit(PartyEvent::Pause),
                        HostEvent::Seek(to) => cx.emit(PartyEvent::Seek(to)),
                    }
                    cx.notify();
                    true
                });
                if !alive.is_ok_and(|alive| alive) {
                    break;
                }
            }
        })
        .detach();
    }

    fn join(&mut self, cx: &mut Context<Self>) {
        let code = clean_code(&self.join_code.read(cx).value());
        if code.chars().count() != CODE_LENGTH {
            return;
        }
        self.leave(cx);
        self.error = None;
        self.connecting = true;
        let session = self.session;
        let name = match self.name(cx) {
            name if name.is_empty() => "Guest".to_string(),
            name => name,
        };
        cx.notify();
        let joining = code.clone();
        cx.spawn(async move |party, cx| {
            let joined =
                runtime::run(async move { Guest::join(&api(), &joining, &name).await }).await;
            let _ = party.update(cx, |party, cx| {
                if party.session != session {
                    return;
                }
                party.connecting = false;
                match joined {
                    Some(Ok((guest, events))) => party.following(guest, code, events, cx),
                    Some(Err(error)) => party.error = Some(format!("{error:#}")),
                    None => party.error = Some("Could not join the party.".into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn following(
        &mut self,
        guest: Guest,
        code: String,
        mut events: tokio::sync::mpsc::UnboundedReceiver<GuestEvent>,
        cx: &mut Context<Self>,
    ) {
        self.role = Role::Guest;
        self.code = Some(code);
        self.guest = Some(Arc::new(guest));
        let session = self.session;
        cx.spawn(async move |party, cx| {
            while let Some(event) = events.recv().await {
                let alive = party.update(cx, |party, cx| {
                    if party.session != session {
                        return false;
                    }
                    party.heard(event, cx)
                });
                if !alive.is_ok_and(|alive| alive) {
                    break;
                }
            }
        })
        .detach();
    }

    /// What the host said. False once the party is over.
    fn heard(&mut self, event: GuestEvent, cx: &mut Context<Self>) -> bool {
        match event {
            GuestEvent::Target(target) => {
                let item = target.playback.item.clone();
                let changed = item != self.item;
                self.item = item.clone();
                self.target = Some(target);
                // What the host starts, the guest is taken to, once. After this update is over: the
                // page it builds reads the party.
                if let (true, Some(Item::Video { id })) = (changed, item) {
                    cx.defer(move |cx| route::navigate(Route::Watch { id }, cx));
                }
            }
            GuestEvent::Members(members) => self.members = members,
            GuestEvent::Ended => {
                self.reset();
                self.error = Some("The party ended.".into());
                cx.notify();
                return false;
            }
        }
        cx.notify();
        true
    }

    fn copy(&mut self, what: Copied, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some(what);
        cx.notify();
        cx.spawn(async move |party, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = party.update(cx, |party, cx| {
                if party.copied == Some(what) {
                    party.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Fills the code from the clipboard.
    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let code = clean_code(&text);
        self.join_code
            .update(cx, |input, cx| input.set_value(code, window, cx));
    }

    /// The start page: who you appear as, and a button.
    fn start(&self, cx: &mut Context<Self>) -> Div {
        let name = self.name.read(cx).value().to_string();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2p5()
                    .rounded_xl()
                    .bg(color(TEXT).opacity(0.04))
                    .child(avatar(&name, false))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(TEXT_FAINT))
                                    .child("You appear as"),
                            )
                            // The field has padding of its own, which would set the name in from its label.
                            .child(div().ml(px(-12.)).child(Input::new(&self.name).appearance(false))),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(relative(1.35))
                    .text_color(color(TEXT_DIM))
                    .child(
                        "Everyone plays what you play, in step. Share the code, and they join in seconds.",
                    ),
            )
            .child(
                action("party-start", "Start party", true, false)
                    .on_click(cx.listener(|party, _, _, cx| party.create(cx))),
            )
    }

    /// The join page: six boxes for the code, over one real field.
    fn join_page(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let typed = self.join_code.read(cx).value().to_string();
        let focused = self.join_code.read(cx).focus_handle(cx).is_focused(window);
        let complete = typed.chars().count() == CODE_LENGTH;
        let caret = typed.chars().count().min(CODE_LENGTH - 1);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(small_label("Party code"))
                            .child(
                                div()
                                    .id("party-paste")
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(TEXT_DIM))
                                    .hover(|style| style.text_color(color(TEXT)))
                                    .on_click(cx.listener(|party, _, window, cx| {
                                        party.paste(window, cx)
                                    }))
                                    .child("Paste"),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .child(code_cells(&typed, focused.then_some(caret), None))
                            // One real field over the boxes, so typing, pasting and the cursor keys
                            // all work as they do anywhere else.
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .opacity(0.)
                                    .child(Input::new(&self.join_code).appearance(false)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(relative(1.35))
                    .text_color(color(TEXT_DIM))
                    .child("Ask the host for their six letter code. You join as soon as it is complete."),
            )
            .child(
                action("party-join", "Join party", false, !complete)
                    .on_click(cx.listener(|party, _, _, cx| party.join(cx))),
            )
    }

    /// In a party: the code to share, who is here, and a way out.
    fn live(&self, cx: &mut Context<Self>) -> Div {
        let code = self.code.clone().unwrap_or_default();
        let host = self.role == Role::Host;
        let link = format!("{SITE}/?party={code}");
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(small_label(if host {
                                "Party code"
                            } else {
                                "You are in party"
                            }))
                            .child(
                                div()
                                    .id("party-copy-link")
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if self.copied == Some(Copied::Link) {
                                        color(ACCENT)
                                    } else {
                                        color(TEXT_DIM)
                                    })
                                    .hover(|style| style.text_color(color(TEXT)))
                                    .on_click(cx.listener(move |party, _, _, cx| {
                                        party.copy(Copied::Link, link.clone(), cx)
                                    }))
                                    .child(if self.copied == Some(Copied::Link) {
                                        "Link copied"
                                    } else {
                                        "Copy link"
                                    }),
                            ),
                    )
                    .child({
                        let copy = code.clone();
                        div()
                            .id("party-copy-code")
                            .cursor_pointer()
                            .on_click(cx.listener(move |party, _, _, cx| {
                                party.copy(Copied::Code, copy.clone(), cx)
                            }))
                            .child(code_cells(
                                &code,
                                None,
                                (self.copied == Some(Copied::Code)).then_some("Copied"),
                            ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(small_label(&format!(
                        "In this party · {}",
                        self.members.len()
                    )))
                    .child(
                        div()
                            .id("party-members")
                            .flex()
                            .flex_col()
                            .max_h(px(224.))
                            .overflow_y_scroll()
                            .children(self.members.iter().map(person)),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(relative(1.35))
                    .text_color(color(TEXT_FAINT))
                    .child(if host {
                        "Anyone can pause and seek. You choose what plays."
                    } else {
                        "The host chooses what plays. You can pause and seek for everyone."
                    }),
            )
            .child(
                div()
                    .id("party-leave")
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(36.))
                    .rounded_lg()
                    .border_1()
                    .border_color(color(LINE))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(TEXT_DIM))
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(color(DANGER).opacity(0.4))
                            .bg(color(DANGER).opacity(0.1))
                            .text_color(color(DANGER))
                    })
                    .on_click(cx.listener(|party, _, _, cx| party.leave(cx)))
                    .child(if host { "End party" } else { "Leave party" }),
            )
    }
}

/// How long "Copied" stays up.
const COPIED_FOR: Duration = Duration::from_millis(1600);

fn small_label(text: &str) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(TEXT_FAINT))
        .child(text.to_string())
}

/// The first letter of a name, in a circle. The host's has a ring.
fn avatar(name: &str, host: bool) -> Div {
    let initial = name
        .trim()
        .chars()
        .next()
        .map_or("?".to_string(), |c| c.to_uppercase().to_string());
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(32.))
        .rounded_full()
        .bg(color(TEXT).opacity(0.08))
        .when(host, |circle| {
            circle.border_1().border_color(color(ACCENT).opacity(0.7))
        })
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .text_color(color(TEXT))
        .child(initial)
}

/// A person in the party, with how they are doing: quiet while all is well, and the host and
/// anyone having trouble are the ones to notice.
fn person(member: &Member) -> Div {
    let status = match member.state {
        Readiness::Trouble => div()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color(DANGER))
            .child("Can't play"),
        Readiness::Loading => div()
            .flex()
            .items_center()
            .gap_1p5()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color(ACCENT))
            .child(div().size(px(6.)).rounded_full().bg(color(ACCENT)))
            .child("Loading"),
        Readiness::Ready if member.host => div()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color(TEXT_FAINT))
            .child("Host"),
        Readiness::Ready => div()
            .size(px(6.))
            .rounded_full()
            .bg(color(TEXT).opacity(0.3)),
    };
    div()
        .flex()
        .items_center()
        .gap_3()
        .py_1p5()
        .child(avatar(&member.name, member.host))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(TEXT))
                .child(member.name.clone()),
        )
        .child(status)
}

/// Six boxes with a code in them. `caret` marks the box being typed into; `over` covers them with a
/// word, for a moment.
fn code_cells(code: &str, caret: Option<usize>, over: Option<&str>) -> Div {
    let chars: Vec<char> = code.chars().collect();
    let mono = if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "monospace"
    };
    div()
        .relative()
        .flex()
        .gap(px(6.))
        .children((0..CODE_LENGTH).map(|index| {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .h(px(48.))
                .rounded_lg()
                .bg(color(TEXT).opacity(0.05))
                .border_1()
                .border_color(if caret == Some(index) {
                    color(ACCENT)
                } else {
                    color(LINE)
                })
                .font_family(mono)
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .text_color(color(TEXT))
                .child(chars.get(index).map(char::to_string).unwrap_or_default())
        }))
        .children(over.map(|word| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(color(SURFACE).opacity(0.92))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(TEXT))
                .child(word.to_string())
        }))
}

/// The panel's one wide button: white to start, quiet to join, and dimmed while it cannot be pressed.
fn action(id: &'static str, label: &str, primary: bool, disabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(44.))
        .rounded_xl()
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(primary, |button| {
            button
                .bg(color(TEXT))
                .text_color(color(CANVAS))
                .hover(|style| style.bg(color(TEXT).opacity(0.85)))
        })
        .when(!primary, |button| {
            button
                .bg(color(TEXT).opacity(0.12))
                .text_color(color(TEXT))
                .hover(|style| style.bg(color(TEXT).opacity(0.2)))
        })
        .when(disabled, |button| button.opacity(0.4))
        .when(!disabled, |button| button.cursor_pointer())
        .child(label.to_string())
}

/// One half of the start / join switch.
fn tab_button(id: &'static str, label: &str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .h(px(32.))
        .rounded_lg()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(active, |tab| {
            tab.bg(color(TEXT).opacity(0.12)).text_color(color(TEXT))
        })
        .when(!active, |tab| {
            tab.text_color(color(TEXT_DIM))
                .hover(|style| style.text_color(color(TEXT)))
        })
        .child(label.to_string())
}

impl Render for Party {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.connecting {
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap_3()
                .py_8()
                .text_size(px(14.))
                .text_color(color(TEXT_DIM))
                .child(Spinner::new())
                .child("Connecting")
        } else if self.role == Role::None {
            let tabs = div()
                .flex()
                .gap(px(2.))
                .p(px(2.))
                .rounded(px(10.))
                .bg(color(TEXT).opacity(0.06))
                .child(
                    tab_button("party-tab-start", "Start", self.tab == Tab::Start).on_click(
                        cx.listener(|party, _, _, cx| {
                            party.tab = Tab::Start;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    tab_button("party-tab-join", "Join", self.tab == Tab::Join).on_click(
                        cx.listener(|party, _, window, cx| {
                            party.tab = Tab::Join;
                            party
                                .join_code
                                .update(cx, |input, cx| input.focus(window, cx));
                            cx.notify();
                        }),
                    ),
                );
            let page = match self.tab {
                Tab::Start => self.start(cx),
                Tab::Join => self.join_page(window, cx),
            };
            div().flex().flex_col().gap_4().child(tabs).child(page)
        } else {
            self.live(cx)
        };

        div()
            .id("party")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|party, _, _, cx| {
                party.panel_open = false;
                cx.notify();
            }))
            .on_key_down(cx.listener(|party, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    party.panel_open = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("party-panel")
                    .absolute()
                    .top(px(TOP_BAR + 6.))
                    .right(px(24.))
                    .w(px(320.))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .rounded_2xl()
                    .bg(color(SURFACE))
                    .border_1()
                    .border_color(color(LINE))
                    .shadow_lg()
                    // A click inside the panel is not a click on the scrim.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(body)
                    .children(
                        self.error
                            .clone()
                            .filter(|_| !self.connecting)
                            .map(|error| {
                                div()
                                    .flex()
                                    .items_start()
                                    .gap_2()
                                    .text_size(px(13.))
                                    .line_height(relative(1.35))
                                    .text_color(color(ACCENT))
                                    .child(
                                        div()
                                            .mt(px(6.))
                                            .flex_none()
                                            .size(px(6.))
                                            .rounded_full()
                                            .bg(color(ACCENT)),
                                    )
                                    .child(div().flex_1().child(error))
                            }),
                    ),
            )
    }
}
