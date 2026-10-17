//! A device that joins a party and follows its host.

use crate::peer::{Open, Peer, Sender, Wire};
use crate::protocol::{Member, Message, Readiness};
use crate::signal;
use crate::sync::{Clock, Target, now_ms, target_of};
use anyhow::bail;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// How long to keep asking for the host's answer: the host answers as soon as it sees the offer,
/// unless its own connection is bad.
const ANSWER_WITHIN: Duration = Duration::from_secs(60);
/// How often the clocks are compared. The first few are quick, so a good reading comes early.
const PING_EVERY: Duration = Duration::from_secs(5);
const QUICK_PINGS: u64 = 5;

#[derive(Debug, Clone)]
pub enum GuestEvent {
    /// What the party should be playing now. Sent when the host changes it and every few seconds.
    Target(Target),
    Members(Vec<Member>),
    /// The host ended the party or the connection to it is gone.
    Ended,
}

pub struct Guest {
    sender: Sender,
    pinger: JoinHandle<()>,
    reader: JoinHandle<()>,
}

impl Guest {
    /// Joins the party `code` on `api`. `name` is how this device appears to the others.
    pub async fn join(
        api: &str,
        code: &str,
        name: &str,
    ) -> anyhow::Result<(Guest, mpsc::UnboundedReceiver<GuestEvent>)> {
        Guest::join_on(api, code, name, "0.0.0.0:0").await
    }

    /// Like `join`, on the local address `bind` (see `Host::create_on`).
    pub async fn join_on(
        api: &str,
        code: &str,
        name: &str,
        bind: &str,
    ) -> anyhow::Result<(Guest, mpsc::UnboundedReceiver<GuestEvent>)> {
        let api = api.trim_end_matches('/');
        let code = code.trim().to_uppercase();
        let ice = signal::ice(api, &code).await?;
        let (peer, offer) = Peer::offering(ice, bind).await?;
        let id = signal::join(api, &code, &offer).await?;
        let answer = wait_for_answer(api, &code, &id).await?;
        peer.accept_answer(&answer).await?;
        let Open { sender, wire } = peer.open().await?;
        sender
            .send(&Message::Hello {
                name: name.to_string(),
            })
            .await;

        let (events, receiver) = mpsc::unbounded_channel();
        let clock = std::sync::Arc::new(std::sync::Mutex::new(Clock::default()));
        let pinger = tokio::spawn({
            let sender = sender.clone();
            async move {
                let mut id = 0u64;
                loop {
                    sender.send(&Message::Ping { id, sent: now_ms() }).await;
                    let wait = if id < QUICK_PINGS {
                        Duration::from_millis(400)
                    } else {
                        PING_EVERY
                    };
                    id += 1;
                    tokio::time::sleep(wait).await;
                }
            }
        });
        let reader = tokio::spawn(read(wire, events, clock));
        Ok((
            Guest {
                sender,
                pinger,
                reader,
            },
            receiver,
        ))
    }

    /// Asks the host to play, pause or seek. The host does it, and everyone, this device
    /// included, hears what it is now.
    pub async fn ask(&self, message: Message) {
        if matches!(
            message,
            Message::Play | Message::Pause | Message::Seek { .. }
        ) {
            self.sender.send(&message).await;
        }
    }

    /// Tells the host how this device is doing with the item whose id is `id`: ready for it,
    /// still loading it, or unable to play it.
    pub async fn status(&self, state: Readiness, id: Option<String>) {
        self.sender.send(&Message::Status { state, id }).await;
    }

    pub async fn leave(&self) {
        self.sender.send(&Message::Bye).await;
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        self.pinger.abort();
        self.reader.abort();
    }
}

async fn wait_for_answer(api: &str, code: &str, guest: &str) -> anyhow::Result<String> {
    let started = std::time::Instant::now();
    while started.elapsed() < ANSWER_WITHIN {
        if let Some(answer) = signal::take_answer(api, code, guest).await? {
            return Ok(answer);
        }
    }
    bail!("the host did not answer")
}

async fn read(
    mut wire: mpsc::UnboundedReceiver<Wire>,
    events: mpsc::UnboundedSender<GuestEvent>,
    clock: std::sync::Arc<std::sync::Mutex<Clock>>,
) {
    // What the host said before the clocks were known is kept, and acted on at the first reading:
    // without one, how long ago it was said cannot be told.
    let mut waiting: Option<Message> = None;
    while let Some(wire) = wire.recv().await {
        let Wire::Message(message) = wire else { break };
        let state = match message {
            state @ Message::State { .. } => Some(state),
            Message::Members { members } => {
                if events.send(GuestEvent::Members(members)).is_err() {
                    return;
                }
                continue;
            }
            Message::Pong { sent, at, .. } => {
                clock.lock().unwrap().sample(sent, at, now_ms());
                waiting.take()
            }
            Message::Bye => break,
            _ => continue,
        };
        let Some(Message::State {
            item,
            playing,
            position,
            at,
        }) = state
        else {
            continue;
        };
        let clock = clock.lock().unwrap();
        if !clock.is_known() {
            waiting = Some(Message::State {
                item,
                playing,
                position,
                at,
            });
            continue;
        }
        let target = target_of(item, playing, position, at, &clock, now_ms());
        if events.send(GuestEvent::Target(target)).is_err() {
            return;
        }
    }
    let _ = events.send(GuestEvent::Ended);
}
