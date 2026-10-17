//! Talking to the meeting point on the API (`/v1/party`, see `desktop/server/API.md`).

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use webrtc::peer_connection::RTCIceServer;

/// The server holds a request for up to 25 seconds; this is a little more.
const LONG_POLL: Duration = Duration::from_secs(40);

#[derive(Deserialize)]
struct IceServer {
    urls: Vec<String>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    credential: String,
}

#[derive(Deserialize)]
struct IceList {
    ice_servers: Vec<IceServer>,
}

fn ice_servers(list: IceList) -> Vec<RTCIceServer> {
    list.ice_servers
        .into_iter()
        .map(|server| RTCIceServer {
            urls: server.urls,
            username: server.username,
            credential: server.credential,
        })
        .collect()
}

pub struct Created {
    pub code: String,
    pub token: String,
}

pub struct Offer {
    pub guest: String,
    pub sdp: String,
}

/// The party is over or never was, or the host's secret is not accepted: asking again is no use.
#[derive(Debug)]
pub struct Gone(pub String);

impl std::fmt::Display for Gone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Gone {}

pub fn is_gone(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Gone>().is_some()
}

/// An error the server explained, or one of the network's.
async fn checked(response: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("the party server answered {status}"));
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Gone(message).into());
    }
    bail!("{message}")
}

/// One client for every call, so a connection to the server is reused between them.
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

pub async fn create(api: &str) -> anyhow::Result<Created> {
    #[derive(Deserialize)]
    struct Body {
        code: String,
        host_token: String,
    }
    let body: Body = checked(
        client()
            .post(format!("{api}/v1/party"))
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .context("could not reach the party server")?,
    )
    .await?
    .json()
    .await?;
    Ok(Created {
        code: body.code,
        token: body.host_token,
    })
}

pub async fn ice(api: &str, code: &str) -> anyhow::Result<Vec<RTCIceServer>> {
    let list: IceList = checked(
        client()
            .get(format!("{api}/v1/party/{code}/ice"))
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .context("could not reach the party server")?,
    )
    .await?
    .json()
    .await?;
    Ok(ice_servers(list))
}

/// Posts a guest's offer and returns the guest's id.
pub async fn join(api: &str, code: &str, offer: &str) -> anyhow::Result<String> {
    let body: serde_json::Value = checked(
        client()
            .post(format!("{api}/v1/party/{code}/join"))
            .json(&json!({ "offer": offer }))
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .context("could not reach the party server")?,
    )
    .await?
    .json()
    .await?;
    Ok(body["guest"].as_str().unwrap_or_default().to_string())
}

/// The offers waiting for the host. Empty when none came within the wait.
pub async fn offers(api: &str, code: &str, token: &str) -> anyhow::Result<Vec<Offer>> {
    let body: serde_json::Value = checked(
        client()
            .get(format!("{api}/v1/party/{code}/offers"))
            .bearer_auth(token)
            .timeout(LONG_POLL)
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    Ok(body["offers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|offer| {
            Some(Offer {
                guest: offer["guest"].as_str()?.to_string(),
                sdp: offer["offer"].as_str()?.to_string(),
            })
        })
        .collect())
}

pub async fn answer(
    api: &str,
    code: &str,
    token: &str,
    guest: &str,
    sdp: &str,
) -> anyhow::Result<()> {
    checked(
        client()
            .post(format!("{api}/v1/party/{code}/answer/{guest}"))
            .bearer_auth(token)
            .json(&json!({ "answer": sdp }))
            .timeout(Duration::from_secs(15))
            .send()
            .await?,
    )
    .await?;
    Ok(())
}

/// The host's answer, or `None` when it has not come within the wait.
pub async fn take_answer(api: &str, code: &str, guest: &str) -> anyhow::Result<Option<String>> {
    let response = checked(
        client()
            .get(format!("{api}/v1/party/{code}/answer/{guest}"))
            .timeout(LONG_POLL)
            .send()
            .await
            .context("could not reach the party server")?,
    )
    .await?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    let body: serde_json::Value = response.json().await?;
    Ok(body["answer"].as_str().map(str::to_string))
}

/// Best effort: a guest has gone, so its place can be given to another.
pub async fn guest_gone(api: &str, code: &str, token: &str, guest: &str) {
    let _ = client()
        .delete(format!("{api}/v1/party/{code}/guests/{guest}"))
        .bearer_auth(token)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
}

/// Best effort: the party is over.
pub async fn end(api: &str, code: &str, token: &str) {
    let _ = client()
        .delete(format!("{api}/v1/party/{code}"))
        .bearer_auth(token)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
}
