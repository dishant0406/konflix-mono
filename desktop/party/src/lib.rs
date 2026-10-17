//! Watch parties: devices that play the same thing together.
//!
//! The host is a device, not a server. Guests connect straight to it with WebRTC, and what is
//! playing travels on that connection. `api.akidev.live` is only asked to pass the first message
//! each way (see `desktop/server/src/party.rs`). The web app speaks the same messages
//! (`akidev-frontend/src/lib/party`), described in `PROTOCOL.md`.

mod guest;
mod host;
mod peer;
mod protocol;
mod signal;
mod sync;

pub use guest::{Guest, GuestEvent};
pub use host::{Host, HostEvent};
pub use protocol::{Item, Member, Message, Readiness};
pub use sync::{Fix, Playback, Target, fix};
