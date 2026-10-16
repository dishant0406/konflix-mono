//! The one outbound HTTP client. Every request gets a timeout and a normal user agent,
//! because several of these hosts reject unknown clients.

use std::sync::OnceLock;
use std::time::Duration;

pub const USER_AGENT: &str = "akidev/2.0";

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            // A host that does not answer at all (a blocked address, a name that will not resolve)
            // is given up on in seconds, not after the whole timeout.
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client builds")
    })
}

/// Reads a response body, failing once it grows past `max` bytes instead of holding all of
/// whatever a host chooses to send.
pub async fn read_capped(mut response: reqwest::Response, max: usize) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(body.len() + chunk.len() <= max, "the response is too large");
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
