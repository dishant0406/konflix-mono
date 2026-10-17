//! The messages devices send each other, as JSON text on one data channel. The web app has the
//! same list in `akidev-frontend/src/lib/party/protocol.ts`; change them together.

use serde::{Deserialize, Serialize};

/// Something to play. Every device finds its own stream from the id, so no address travels.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// A film or an episode: the media id, like `series-tt0903747-s1e5`.
    Video { id: String },
    /// A music track: the video id on YouTube Music, with what a guest shows before it has loaded.
    Music {
        id: String,
        title: String,
        artist: String,
        #[serde(default)]
        cover: Option<String>,
    },
}

impl Item {
    /// The media id or the track id, whichever this is.
    pub fn id(&self) -> &str {
        match self {
            Item::Video { id } | Item::Music { id, .. } => id,
        }
    }
}

/// How a guest is doing with what the party is playing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    /// Can play it now.
    Ready,
    /// Still finding a source or buffering: a film from a torrent can take a while.
    Loading,
    /// Could not find or play it.
    Trouble,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Member {
    pub name: String,
    pub host: bool,
    pub state: Readiness,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Message {
    /// Guest to host, first: who is joining.
    Hello {
        name: String,
    },
    /// Host to guests: what is playing. `at` is the host's clock in milliseconds since 1970,
    /// when `position` (seconds) was read.
    State {
        item: Option<Item>,
        playing: bool,
        position: f64,
        at: u64,
    },
    /// Host to guests: who is in the party.
    Members {
        members: Vec<Member>,
    },
    /// Guest to host: asks the host to do what a person at the host would. The host does it and
    /// tells everyone with a new `State`. What plays is the host's choice, so there is no request
    /// to open something.
    Play,
    Pause,
    Seek {
        position: f64,
    },
    /// Guest to host: how the guest is doing with the item whose id is `id` (a guest that has not
    /// yet said anything about the item the host is playing is counted as loading it).
    Status {
        state: Readiness,
        id: Option<String>,
    },
    /// Guest to host, and the answer: measures the way and the difference between the two clocks.
    Ping {
        id: u64,
        sent: u64,
    },
    Pong {
        id: u64,
        sent: u64,
        at: u64,
    },
    /// Either way: leaving. From the host it ends the party.
    Bye,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web app reads and writes exactly this text.
    #[test]
    fn messages_look_the_way_the_web_app_expects() {
        let state = Message::State {
            item: Some(Item::Video {
                id: "movie-tt1".into(),
            }),
            playing: true,
            position: 12.5,
            at: 1_700_000_000_000,
        };
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            r#"{"t":"state","item":{"kind":"video","id":"movie-tt1"},"playing":true,"position":12.5,"at":1700000000000}"#
        );
        assert_eq!(
            serde_json::to_string(&Message::Pause).unwrap(),
            r#"{"t":"pause"}"#
        );
        assert_eq!(
            serde_json::to_string(&Message::Seek { position: 3.0 }).unwrap(),
            r#"{"t":"seek","position":3.0}"#
        );
        assert_eq!(
            serde_json::to_string(&Message::Status {
                state: Readiness::Loading,
                id: Some("movie-tt1".into())
            })
            .unwrap(),
            r#"{"t":"status","state":"loading","id":"movie-tt1"}"#
        );
        assert_eq!(
            serde_json::to_string(&Member {
                name: "Sam".into(),
                host: false,
                state: Readiness::Trouble
            })
            .unwrap(),
            r#"{"name":"Sam","host":false,"state":"trouble"}"#
        );
        let music: Message = serde_json::from_str(
            r#"{"t":"state","item":{"kind":"music","id":"abc","title":"A","artist":"B"},"playing":false,"position":0.0,"at":1}"#,
        )
        .unwrap();
        assert_eq!(
            music,
            Message::State {
                item: Some(Item::Music {
                    id: "abc".into(),
                    title: "A".into(),
                    artist: "B".into(),
                    cover: None
                }),
                playing: false,
                position: 0.0,
                at: 1
            }
        );
    }
}
