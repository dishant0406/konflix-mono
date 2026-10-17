//! One WebRTC connection with one data channel, the same for both ends.

use crate::protocol::Message;
use anyhow::{Context, anyhow};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState, RTCSessionDescription,
};

/// How long finding the way to the other device (gathering addresses, asking STUN) may take.
const GATHER_FOR: Duration = Duration::from_secs(10);
/// How long the other device has to open the channel once the answer is in.
const OPEN_WITHIN: Duration = Duration::from_secs(30);

/// What the data channel reports.
pub enum Wire {
    Message(Message),
    /// The channel or the connection is gone.
    Closed,
}

struct Events {
    gathered: mpsc::UnboundedSender<()>,
    state: mpsc::UnboundedSender<RTCPeerConnectionState>,
    channel: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathered.send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let _ = self.state.send(state);
    }

    // The answering end is handed the channel the other end made. It is only passed on: the
    // connection's driver waits for this to return.
    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.channel.send(channel);
    }
}

pub struct Peer {
    pc: Arc<dyn PeerConnection>,
    gathered: mpsc::UnboundedReceiver<()>,
    state: mpsc::UnboundedReceiver<RTCPeerConnectionState>,
    channels: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    /// Lets the offering end put its own channel where `open` finds it.
    channel_in: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}

/// The two ends of an open channel.
pub struct Open {
    pub sender: Sender,
    pub wire: mpsc::UnboundedReceiver<Wire>,
}

#[derive(Clone)]
pub struct Sender {
    channel: Arc<dyn DataChannel>,
}

impl Sender {
    pub async fn send(&self, message: &Message) {
        let Ok(text) = serde_json::to_string(message) else {
            return;
        };
        if let Err(error) = self.channel.send_text(&text).await {
            tracing::debug!("[party] could not send: {error}");
        }
    }
}

async fn new_peer(ice: Vec<RTCIceServer>, bind: &str) -> anyhow::Result<Peer> {
    let (gathered_tx, gathered) = mpsc::unbounded_channel();
    let (state_tx, state) = mpsc::unbounded_channel();
    let (channel_in, channels) = mpsc::unbounded_channel();
    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().with_ice_servers(ice).build())
        .with_handler(Arc::new(Events {
            gathered: gathered_tx,
            state: state_tx,
            channel: channel_in.clone(),
        }))
        .with_udp_addrs(vec![bind.to_string()])
        .build()
        .await
        .map_err(|error| anyhow!("could not start the connection: {error}"))?;
    Ok(Peer {
        pc: Arc::new(pc),
        gathered,
        state,
        channels,
        channel_in,
    })
}

impl Peer {
    /// A connection that makes the offer (a guest). `bind` is the local address to use, which is
    /// `0.0.0.0:0` (every interface) except in tests.
    pub async fn offering(ice: Vec<RTCIceServer>, bind: &str) -> anyhow::Result<(Peer, String)> {
        let mut peer = new_peer(ice, bind).await?;
        let channel = peer
            .pc
            .create_data_channel("party", None)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        let _ = peer.channel_in.send(channel);
        let offer = peer
            .pc
            .create_offer(None)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        peer.pc
            .set_local_description(offer)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        let sdp = peer.local_sdp().await?;
        Ok((peer, sdp))
    }

    /// A connection that answers an offer (the host).
    pub async fn answering(
        ice: Vec<RTCIceServer>,
        bind: &str,
        offer: &str,
    ) -> anyhow::Result<(Peer, String)> {
        let mut peer = new_peer(ice, bind).await?;
        let offer = RTCSessionDescription::offer(offer.to_string())
            .map_err(|_| anyhow!("that is not a connection offer"))?;
        peer.pc
            .set_remote_description(offer)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        let answer = peer
            .pc
            .create_answer(None)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        peer.pc
            .set_local_description(answer)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        let sdp = peer.local_sdp().await?;
        Ok((peer, sdp))
    }

    /// The description once every address has been found, so it can be sent whole.
    async fn local_sdp(&mut self) -> anyhow::Result<String> {
        // Gathering that has not finished by now is sent as it is: it has the addresses found.
        let _ = tokio::time::timeout(GATHER_FOR, self.gathered.recv()).await;
        let description = self
            .pc
            .local_description()
            .await
            .context("no connection description")?;
        Ok(description.sdp)
    }

    /// The offering end takes the other's answer.
    pub async fn accept_answer(&self, answer: &str) -> anyhow::Result<()> {
        let answer = RTCSessionDescription::answer(answer.to_string())
            .map_err(|_| anyhow!("that is not a connection answer"))?;
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    /// Waits for the channel to open, then reports what comes over it. The connection failing or
    /// closing is reported as `Wire::Closed`, and the connection is closed when the caller lets go
    /// of what comes over it.
    pub async fn open(self) -> anyhow::Result<Open> {
        let Peer {
            pc,
            mut channels,
            mut state,
            ..
        } = self;
        let channel = match opened_channel(&mut channels).await {
            Ok(channel) => channel,
            Err(error) => {
                let _ = pc.close().await;
                return Err(error);
            }
        };
        let (wire_tx, wire) = mpsc::unbounded_channel();
        let reader = channel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = reader.poll() => match event {
                        Some(DataChannelEvent::OnMessage(message)) => {
                            let text = String::from_utf8_lossy(&message.data).into_owned();
                            match serde_json::from_str::<Message>(&text) {
                                Ok(message) => {
                                    if wire_tx.send(Wire::Message(message)).is_err() {
                                        break;
                                    }
                                }
                                Err(error) => tracing::debug!("[party] ignored a message: {error}"),
                            }
                        }
                        Some(DataChannelEvent::OnClose) | None => break,
                        _ => {}
                    },
                    state = state.recv() => match state {
                        // Disconnected can mend itself; Failed cannot.
                        Some(RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed)
                            | None => break,
                        _ => {}
                    },
                    // Nobody is listening any more: the party was left or ended.
                    () = wire_tx.closed() => break,
                }
            }
            let _ = wire_tx.send(Wire::Closed);
            let _ = pc.close().await;
        });
        Ok(Open {
            sender: Sender { channel },
            wire,
        })
    }
}

/// The data channel, once it has opened.
async fn opened_channel(
    channels: &mut mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
) -> anyhow::Result<Arc<dyn DataChannel>> {
    let channel = tokio::time::timeout(OPEN_WITHIN, channels.recv())
        .await
        .context("the other device did not connect")?
        .context("the connection closed")?;
    let opening = channel.clone();
    tokio::time::timeout(OPEN_WITHIN, async move {
        loop {
            match opening.poll().await {
                Some(DataChannelEvent::OnOpen) => return Ok(()),
                Some(DataChannelEvent::OnClose) | None => {
                    return Err(anyhow!("the connection closed"));
                }
                _ => {}
            }
        }
    })
    .await
    .context("the other device did not connect")??;
    Ok(channel)
}
