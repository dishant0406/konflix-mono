//! The device that holds the party. Guests connect to it; it tells them what is playing.

use crate::peer::{Open, Peer, Sender, Wire};
use crate::protocol::{Member, Message, Readiness};
use crate::signal::{self, is_gone};
use crate::sync::{Playback, now_ms};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

/// What a guest asked for. The host does it, as if a person at the host had.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    Play,
    Pause,
    Seek(f64),
    /// Someone joined or left, or said they are having trouble.
    Members(Vec<Member>),
    /// The party server stopped answering for the host, so no one new can join.
    Unreachable,
}

struct GuestSlot {
    sender: Sender,
    name: String,
    /// What the guest last said about how it is doing, and about which item.
    state: Readiness,
    about: Option<String>,
}

struct Shared {
    api: String,
    code: String,
    token: String,
    name: String,
    guests: Mutex<HashMap<String, GuestSlot>>,
    /// What was last published, so a guest who joins late is told at once.
    last: Mutex<Option<Message>>,
    events: mpsc::UnboundedSender<HostEvent>,
    bind: String,
    /// Woken when the party ends, so what looks after each guest lets go.
    ending: Notify,
}

pub struct Host {
    pub code: String,
    shared: Arc<Shared>,
    offers: JoinHandle<()>,
}

/// Failing to reach the party server this many times in a row ends the wait for guests.
const GIVE_UP_AFTER: u32 = 6;

impl Host {
    /// Opens a party on `api`. `name` is how the host appears to the guests.
    pub async fn create(
        api: &str,
        name: &str,
    ) -> anyhow::Result<(Host, mpsc::UnboundedReceiver<HostEvent>)> {
        Host::create_on(api, name, "0.0.0.0:0").await
    }

    /// Like `create`, on the local address `bind` (an address and port, `0` for any) instead of
    /// every interface. The tests use it to connect two devices on one machine.
    pub async fn create_on(
        api: &str,
        name: &str,
        bind: &str,
    ) -> anyhow::Result<(Host, mpsc::UnboundedReceiver<HostEvent>)> {
        let created = signal::create(api).await?;
        let (events, receiver) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            api: api.trim_end_matches('/').to_string(),
            code: created.code.clone(),
            token: created.token,
            name: name.to_string(),
            guests: Mutex::default(),
            last: Mutex::default(),
            events,
            bind: bind.to_string(),
            ending: Notify::new(),
        });
        let offers = tokio::spawn(take_offers(shared.clone()));
        Ok((
            Host {
                code: created.code,
                shared,
                offers,
            },
            receiver,
        ))
    }

    /// What is playing now. Called when it changes and every couple of seconds while it plays.
    pub async fn publish(&self, playback: Playback) {
        let message = Message::State {
            item: playback.item,
            playing: playback.playing,
            position: playback.position,
            at: now_ms(),
        };
        let changed = {
            let mut last = self.shared.last.lock().unwrap();
            let before = last.as_ref().and_then(item_id);
            let changed = before != item_id(&message);
            *last = Some(message.clone());
            changed
        };
        send_all(&self.shared, &message).await;
        // A guest is loading whatever the host starts until it says it is ready for it, so a new
        // item changes who is.
        if changed {
            share_members(&self.shared).await;
        }
    }

    /// Ends the party: guests are told, and the party is closed on the server.
    pub async fn end(&self) {
        self.offers.abort();
        send_all(&self.shared, &Message::Bye).await;
        self.shared.ending.notify_waiters();
        signal::end(&self.shared.api, &self.shared.code, &self.shared.token).await;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.offers.abort();
    }
}

fn senders(shared: &Shared) -> Vec<Sender> {
    shared
        .guests
        .lock()
        .unwrap()
        .values()
        .map(|slot| slot.sender.clone())
        .collect()
}

async fn send_all(shared: &Shared, message: &Message) {
    for sender in senders(shared) {
        sender.send(message).await;
    }
}

/// The id of what a `State` message is playing.
fn item_id(message: &Message) -> Option<String> {
    match message {
        Message::State { item, .. } => item.as_ref().map(|item| item.id().to_string()),
        _ => None,
    }
}

fn members(shared: &Shared) -> Vec<Member> {
    let playing = shared.last.lock().unwrap().as_ref().and_then(item_id);
    let guests = shared.guests.lock().unwrap();
    let mut list = vec![Member {
        name: shared.name.clone(),
        host: true,
        state: Readiness::Ready,
    }];
    list.extend(guests.values().map(|slot| Member {
        name: slot.name.clone(),
        host: false,
        state: guest_state(slot.state, slot.about.as_deref(), playing.as_deref()),
    }));
    list
}

/// How a guest is doing with what the host is playing now. What it said about an earlier item says
/// nothing about this one: it is still getting to it.
fn guest_state(said: Readiness, about: Option<&str>, playing: Option<&str>) -> Readiness {
    match playing {
        Some(id) if about != Some(id) => Readiness::Loading,
        _ => said,
    }
}

/// Tells the guests and the host's own screen who is in the party.
async fn share_members(shared: &Shared) {
    let members = members(shared);
    send_all(
        shared,
        &Message::Members {
            members: members.clone(),
        },
    )
    .await;
    let _ = shared.events.send(HostEvent::Members(members));
}

/// Asks the server for guests, one after another, and connects each as it comes.
async fn take_offers(shared: Arc<Shared>) {
    let mut failures = 0;
    loop {
        match signal::offers(&shared.api, &shared.code, &shared.token).await {
            Ok(offers) => {
                failures = 0;
                for offer in offers {
                    tokio::spawn(accept(shared.clone(), offer));
                }
            }
            // The server no longer has the party (it was restarted, or the host was away too
            // long): no one new can join, and the host is told.
            Err(error) if is_gone(&error) => {
                let _ = shared.events.send(HostEvent::Unreachable);
                break;
            }
            Err(error) => {
                failures += 1;
                tracing::debug!("[party] asking for guests failed: {error:#}");
                if failures >= GIVE_UP_AFTER {
                    let _ = shared.events.send(HostEvent::Unreachable);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

/// Answers one guest's offer and looks after that guest until they are gone.
async fn accept(shared: Arc<Shared>, offer: signal::Offer) {
    let guest = offer.guest;
    let result = connect(&shared, &guest, &offer.sdp).await;
    match result {
        Ok(open) => look_after(&shared, &guest, open).await,
        Err(error) => tracing::debug!("[party] a guest could not connect: {error:#}"),
    }
    // Whatever happened, the place is free again.
    let left = shared.guests.lock().unwrap().remove(&guest).is_some();
    signal::guest_gone(&shared.api, &shared.code, &shared.token, &guest).await;
    if left {
        share_members(&shared).await;
    }
}

async fn connect(shared: &Shared, guest: &str, offer: &str) -> anyhow::Result<Open> {
    // Asked for each time: the relay's login lasts a few hours, and a party can last longer.
    let ice = signal::ice(&shared.api, &shared.code).await?;
    let (peer, answer) = Peer::answering(ice, &shared.bind, offer).await?;
    signal::answer(&shared.api, &shared.code, &shared.token, guest, &answer).await?;
    peer.open().await
}

async fn look_after(shared: &Shared, guest: &str, mut open: Open) {
    shared.guests.lock().unwrap().insert(
        guest.to_string(),
        GuestSlot {
            sender: open.sender.clone(),
            name: "Guest".to_string(),
            state: Readiness::Ready,
            about: None,
        },
    );
    // A guest who joins late is told what is playing, as it is now, before anything else.
    let last = shared.last.lock().unwrap().clone();
    if let Some(state) = last {
        open.sender.send(&as_of_now(state)).await;
    }
    share_members(shared).await;

    loop {
        let wire = tokio::select! {
            wire = open.wire.recv() => wire,
            () = shared.ending.notified() => None,
        };
        let Some(Wire::Message(message)) = wire else {
            break;
        };
        match message {
            Message::Hello { name } => {
                if let Some(slot) = shared.guests.lock().unwrap().get_mut(guest) {
                    slot.name = clean_name(&name);
                }
                share_members(shared).await;
            }
            Message::Status { state, id } => {
                if let Some(slot) = shared.guests.lock().unwrap().get_mut(guest) {
                    slot.state = state;
                    slot.about = id;
                }
                share_members(shared).await;
            }
            Message::Ping { id, sent } => {
                open.sender
                    .send(&Message::Pong {
                        id,
                        sent,
                        at: now_ms(),
                    })
                    .await;
            }
            Message::Play => {
                let _ = shared.events.send(HostEvent::Play);
            }
            Message::Pause => {
                let _ = shared.events.send(HostEvent::Pause);
            }
            Message::Seek { position } if position.is_finite() => {
                let _ = shared.events.send(HostEvent::Seek(position.max(0.0)));
            }
            Message::Bye => break,
            _ => {}
        }
    }
}

/// A published state, moved on to now: a playing one has been playing since it was said.
fn as_of_now(state: Message) -> Message {
    match state {
        Message::State {
            item,
            playing: true,
            position,
            at,
        } => {
            let now = now_ms();
            Message::State {
                item,
                playing: true,
                position: position + now.saturating_sub(at) as f64 / 1000.0,
                at: now,
            }
        }
        other => other,
    }
}

/// A name from a guest is shown to everyone, so it is kept short and plain.
fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
    let name = name.trim();
    if name.is_empty() {
        "Guest".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guest_is_loading_what_it_has_not_yet_said_it_is_ready_for() {
        use Readiness::{Loading, Ready, Trouble};
        // Nothing playing: whatever the guest said stands.
        assert_eq!(guest_state(Ready, None, None), Ready);
        // Something is playing and the guest has said nothing, or said it of the one before.
        assert_eq!(guest_state(Ready, None, Some("b")), Loading);
        assert_eq!(guest_state(Ready, Some("a"), Some("b")), Loading);
        // Said of this one.
        assert_eq!(guest_state(Ready, Some("b"), Some("b")), Ready);
        assert_eq!(guest_state(Loading, Some("b"), Some("b")), Loading);
        assert_eq!(guest_state(Trouble, Some("b"), Some("b")), Trouble);
    }

    #[test]
    fn a_state_told_to_a_late_guest_is_moved_on_to_now() {
        let said = Message::State {
            item: None,
            playing: true,
            position: 10.0,
            at: now_ms() - 5_000,
        };
        let Message::State { position, .. } = as_of_now(said) else {
            panic!("still a state")
        };
        assert!((position - 15.0).abs() < 0.5);
        let paused = Message::State {
            item: None,
            playing: false,
            position: 10.0,
            at: 1,
        };
        assert_eq!(as_of_now(paused.clone()), paused);
    }

    #[test]
    fn a_guests_name_is_kept_short_and_plain() {
        assert_eq!(clean_name("  Sam \n"), "Sam");
        assert_eq!(clean_name(""), "Guest");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), 24);
    }
}
