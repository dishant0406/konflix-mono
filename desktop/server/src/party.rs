//! The meeting point of a watch party.
//!
//! A party's host is a device (the desktop app or a browser tab). Guests connect to it directly,
//! with WebRTC, and everything that is played travels on that connection. All this server does is
//! pass the first message each way, the offer and the answer, so two devices that cannot see each
//! other can find each other. It never sees what is playing, and a party is forgotten as soon as
//! its host stops asking for guests.
//!
//! A guest posts its offer; the host takes the offers waiting for it (a request that waits until
//! there is one), and posts an answer to each; the guest takes its answer the same way.

use crate::config::Settings;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha1::Sha1;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;

/// Letters and digits that are not easily mistaken for each other.
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;
/// Parties held at once, and guests in one. Every guest is a connection to the host's device.
pub const MAX_PARTIES: usize = 5000;
pub const MAX_GUESTS: usize = 20;
/// An offer or answer is a few kilobytes; this is far more than any real one.
pub const MAX_SDP: usize = 32 * 1024;
/// A party whose host has not asked for guests for this long is gone. A host asks continuously.
const HOST_SILENT_FOR: Duration = Duration::from_secs(120);
/// No party outlives this, whatever its host does.
const LONGEST: Duration = Duration::from_secs(12 * 60 * 60);
/// How long a request for the next offer or answer waits before saying there is none yet.
pub const WAIT: Duration = Duration::from_secs(25);
/// A guest's place is held for the host this long before the host has been given the offer.
const UNTAKEN_FOR: Duration = Duration::from_secs(60);
/// How long the relay's credentials last.
const RELAY_CREDENTIALS_FOR: Duration = Duration::from_secs(6 * 60 * 60);

pub struct Offer {
    pub guest: String,
    pub sdp: String,
}

/// A place a guest holds in a party.
struct Issued {
    at: Instant,
    /// The host has been given the guest's offer, so the host looks after the place from here.
    taken: bool,
}

struct Party {
    host_token: String,
    created: Instant,
    host_seen: Instant,
    offers: Vec<Offer>,
    answers: HashMap<String, String>,
    /// The guests who have a place, by id.
    issued: HashMap<String, Issued>,
    /// Woken when this party gets an offer or an answer or ends. Waiting requests then look again.
    changed: Arc<Notify>,
}

impl Party {
    /// A guest whose offer the host never received (the reply to the host's request was lost)
    /// would hold a place for good, so places the host has not taken in a minute are given back.
    fn forget_untaken(&mut self, now: Instant) {
        self.issued
            .retain(|_, place| place.taken || now.duration_since(place.at) < UNTAKEN_FOR);
        let issued = &self.issued;
        self.offers
            .retain(|offer| issued.contains_key(&offer.guest));
    }
}

#[derive(Default)]
pub struct Parties {
    held: Mutex<HashMap<String, Party>>,
}

pub struct Created {
    pub code: String,
    pub host_token: String,
}

/// Why the host's answer was refused.
#[derive(Debug, PartialEq)]
pub enum Refused {
    /// The party is over, or never was.
    NoParty,
    NotTheHost,
    /// No guest has that id (any more).
    NoGuest,
}

fn random_text(length: usize, alphabet: &[u8]) -> String {
    let mut bytes = vec![0u8; length];
    getrandom::fill(&mut bytes).expect("the system has randomness");
    bytes
        .iter()
        .map(|byte| alphabet[*byte as usize % alphabet.len()] as char)
        .collect()
}

fn forget_the_gone(held: &mut HashMap<String, Party>, now: Instant) {
    held.retain(|_, party| {
        now.duration_since(party.host_seen) < HOST_SILENT_FOR
            && now.duration_since(party.created) < LONGEST
    });
}

impl Parties {
    /// Starts a party, or `None` when the server is holding as many as it will.
    pub fn create(&self) -> Option<Created> {
        let now = Instant::now();
        let mut held = self.held.lock().unwrap();
        forget_the_gone(&mut held, now);
        if held.len() >= MAX_PARTIES {
            return None;
        }
        let code = loop {
            let code = random_text(CODE_LENGTH, ALPHABET);
            if !held.contains_key(&code) {
                break code;
            }
        };
        let host_token = random_text(32, b"abcdefghijklmnopqrstuvwxyz0123456789");
        held.insert(
            code.clone(),
            Party {
                host_token: host_token.clone(),
                created: now,
                host_seen: now,
                offers: Vec::new(),
                answers: HashMap::new(),
                issued: HashMap::new(),
                changed: Arc::new(Notify::new()),
            },
        );
        Some(Created { code, host_token })
    }

    pub fn exists(&self, code: &str) -> bool {
        let mut held = self.held.lock().unwrap();
        forget_the_gone(&mut held, Instant::now());
        held.contains_key(code)
    }

    /// A guest's offer. Returns the guest's id, or `None` when there is no such party or it is full.
    pub fn join(&self, code: &str, sdp: String) -> Option<String> {
        let now = Instant::now();
        let mut held = self.held.lock().unwrap();
        forget_the_gone(&mut held, now);
        let party = held.get_mut(code)?;
        party.forget_untaken(now);
        if party.issued.len() >= MAX_GUESTS {
            return None;
        }
        let guest = random_text(12, b"abcdefghijklmnopqrstuvwxyz0123456789");
        party.issued.insert(
            guest.clone(),
            Issued {
                at: now,
                taken: false,
            },
        );
        party.offers.push(Offer {
            guest: guest.clone(),
            sdp,
        });
        party.changed.notify_waiters();
        Some(guest)
    }

    /// A guest left or failed to connect, so there is room for another. Says nothing twice.
    pub fn guest_gone(&self, code: &str, guest: &str) {
        if let Some(party) = self.held.lock().unwrap().get_mut(code) {
            party.issued.remove(guest);
            party.answers.remove(guest);
            party.offers.retain(|offer| offer.guest != guest);
        }
    }

    pub fn is_host(&self, code: &str, token: &str) -> bool {
        self.held
            .lock()
            .unwrap()
            .get(code)
            .is_some_and(|party| party.host_token == token)
    }

    /// The offers waiting for the host, waiting up to `WAIT` for the first. `None` means the
    /// token is wrong or the party is gone.
    pub async fn take_offers(&self, code: &str, token: &str) -> Option<Vec<Offer>> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let changed = self.changes_of(code, token)?;
            let woken = changed.notified();
            tokio::pin!(woken);
            // Registered before looking, so an offer that arrives in between is not missed.
            woken.as_mut().enable();
            {
                let now = Instant::now();
                let mut held = self.held.lock().unwrap();
                forget_the_gone(&mut held, now);
                let party = held.get_mut(code).filter(|p| p.host_token == token)?;
                party.host_seen = now;
                party.forget_untaken(now);
                if !party.offers.is_empty() {
                    for offer in &party.offers {
                        if let Some(place) = party.issued.get_mut(&offer.guest) {
                            place.taken = true;
                        }
                    }
                    return Some(std::mem::take(&mut party.offers));
                }
            }
            if tokio::time::timeout_at(deadline, woken).await.is_err() {
                return Some(Vec::new());
            }
        }
    }

    /// What wakes the requests waiting on a party, if the token is its host's.
    fn changes_of(&self, code: &str, token: &str) -> Option<Arc<Notify>> {
        let held = self.held.lock().unwrap();
        let party = held.get(code).filter(|party| party.host_token == token)?;
        Some(party.changed.clone())
    }

    /// The host's answer to a guest's offer.
    pub fn answer(&self, code: &str, token: &str, guest: &str, sdp: String) -> Result<(), Refused> {
        let mut held = self.held.lock().unwrap();
        forget_the_gone(&mut held, Instant::now());
        let party = held.get_mut(code).ok_or(Refused::NoParty)?;
        if party.host_token != token {
            return Err(Refused::NotTheHost);
        }
        if !party.issued.contains_key(guest) {
            return Err(Refused::NoGuest);
        }
        party.answers.insert(guest.to_string(), sdp);
        party.changed.notify_waiters();
        Ok(())
    }

    /// The answer to a guest's offer, waiting up to `WAIT` for it. `None` means the party is gone,
    /// or no such guest asked.
    pub async fn take_answer(&self, code: &str, guest: &str) -> Option<Option<String>> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let changed = {
                let held = self.held.lock().unwrap();
                let party = held.get(code).filter(|p| p.issued.contains_key(guest))?;
                party.changed.clone()
            };
            let woken = changed.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            {
                let mut held = self.held.lock().unwrap();
                forget_the_gone(&mut held, Instant::now());
                let party = held
                    .get_mut(code)
                    .filter(|p| p.issued.contains_key(guest))?;
                if let Some(answer) = party.answers.remove(guest) {
                    return Some(Some(answer));
                }
            }
            if tokio::time::timeout_at(deadline, woken).await.is_err() {
                return Some(None);
            }
        }
    }

    /// Ends a party. False when the token is wrong.
    pub fn end(&self, code: &str, token: &str) -> bool {
        let mut held = self.held.lock().unwrap();
        if held.get(code).is_none_or(|party| party.host_token != token) {
            return false;
        }
        if let Some(party) = held.remove(code) {
            party.changed.notify_waiters();
        }
        true
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// The servers a device uses to find a path to the other one: the public STUN servers, and the
/// relay when one is set up. The relay's login is made for one party and stops working after a
/// few hours (the scheme coturn's `use-auth-secret` expects).
pub fn ice_servers(settings: &Settings, code: &str) -> Vec<IceServer> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    ice_servers_at(settings, code, now)
}

fn ice_servers_at(settings: &Settings, code: &str, now: u64) -> Vec<IceServer> {
    let mut servers = Vec::new();
    if !settings.stun_urls.is_empty() {
        servers.push(IceServer {
            urls: settings.stun_urls.clone(),
            username: None,
            credential: None,
        });
    }
    if !settings.turn_urls.is_empty() && !settings.turn_secret.is_empty() {
        let username = format!("{}:{code}", now + RELAY_CREDENTIALS_FOR.as_secs());
        let mut mac = Hmac::<Sha1>::new_from_slice(settings.turn_secret.as_bytes())
            .expect("HMAC takes a key of any length");
        mac.update(username.as_bytes());
        servers.push(IceServer {
            urls: settings.turn_urls.clone(),
            username: Some(username),
            credential: Some(
                base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
            ),
        });
    }
    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_offer_reaches_the_host_and_the_answer_reaches_the_guest() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        let guest = parties.join(&party.code, "offer".into()).unwrap();

        let offers = parties
            .take_offers(&party.code, &party.host_token)
            .await
            .unwrap();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            (offers[0].guest.as_str(), offers[0].sdp.as_str()),
            (guest.as_str(), "offer")
        );

        assert_eq!(
            parties.answer(&party.code, &party.host_token, &guest, "answer".into()),
            Ok(())
        );
        assert_eq!(
            parties.take_answer(&party.code, &guest).await,
            Some(Some("answer".to_string()))
        );
    }

    #[tokio::test]
    async fn the_host_waits_for_an_offer_that_has_not_come_yet() {
        let parties = std::sync::Arc::new(Parties::default());
        let party = parties.create().unwrap();
        let waiting = {
            let (parties, code, token) = (parties.clone(), party.code.clone(), party.host_token);
            tokio::spawn(async move { parties.take_offers(&code, &token).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        parties.join(&party.code, "late".into()).unwrap();
        let offers = waiting.await.unwrap().unwrap();
        assert_eq!(offers[0].sdp, "late");
    }

    #[tokio::test]
    async fn another_partys_news_does_not_wake_a_waiting_host() {
        let parties = std::sync::Arc::new(Parties::default());
        let (ours, other) = (parties.create().unwrap(), parties.create().unwrap());
        let waiting = {
            let (parties, code, token) = (parties.clone(), ours.code.clone(), ours.host_token);
            tokio::spawn(async move { parties.take_offers(&code, &token).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        parties.join(&other.code, "not ours".into()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished());
        waiting.abort();
    }

    #[test]
    fn only_the_host_can_answer_or_end_a_party() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        let guest = parties.join(&party.code, "offer".into()).unwrap();
        assert_eq!(
            parties.answer(&party.code, "wrong", &guest, "x".into()),
            Err(Refused::NotTheHost)
        );
        assert!(!parties.end(&party.code, "wrong"));
        assert!(parties.exists(&party.code));
        assert!(parties.end(&party.code, &party.host_token));
        assert!(!parties.exists(&party.code));
        assert!(parties.join(&party.code, "offer".into()).is_none());
        assert_eq!(
            parties.answer(&party.code, &party.host_token, &guest, "x".into()),
            Err(Refused::NoParty)
        );
    }

    #[tokio::test]
    async fn an_answer_for_a_guest_that_never_asked_is_refused() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        assert_eq!(
            parties.answer(&party.code, &party.host_token, "nobody", "x".into()),
            Err(Refused::NoGuest)
        );
        // Nor does waiting for one hold a request.
        assert_eq!(parties.take_answer(&party.code, "nobody").await, None);
    }

    #[test]
    fn a_party_has_room_for_so_many_guests() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        let mut guests = Vec::new();
        for _ in 0..MAX_GUESTS {
            guests.push(parties.join(&party.code, "offer".into()).unwrap());
        }
        assert!(parties.join(&party.code, "offer".into()).is_none());
        parties.guest_gone(&party.code, &guests[0]);
        // Saying so twice does not give a second place back.
        parties.guest_gone(&party.code, &guests[0]);
        assert!(parties.join(&party.code, "offer".into()).is_some());
        assert!(parties.join(&party.code, "offer".into()).is_none());
    }

    #[test]
    fn a_place_the_host_never_heard_of_is_given_back() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        for _ in 0..MAX_GUESTS {
            parties.join(&party.code, "offer".into()).unwrap();
        }
        for place in parties
            .held
            .lock()
            .unwrap()
            .get_mut(&party.code)
            .unwrap()
            .issued
            .values_mut()
        {
            place.at = Instant::now() - UNTAKEN_FOR - Duration::from_secs(1);
        }
        assert!(parties.join(&party.code, "offer".into()).is_some());
    }

    #[test]
    fn a_party_whose_host_went_quiet_is_forgotten() {
        let parties = Parties::default();
        let party = parties.create().unwrap();
        parties
            .held
            .lock()
            .unwrap()
            .get_mut(&party.code)
            .unwrap()
            .host_seen = Instant::now() - HOST_SILENT_FOR - Duration::from_secs(1);
        assert!(!parties.exists(&party.code));
    }

    #[test]
    fn the_relay_login_is_made_from_the_secret_and_the_party() {
        let mut settings = Settings::from_env();
        settings.turn_urls = vec!["turn:relay.example:3478".into()];
        settings.turn_secret = "secret".into();
        let servers = ice_servers_at(&settings, "ABC234", 1_000);
        assert_eq!(servers.len(), 2);
        let relay = &servers[1];
        assert_eq!(relay.username.as_deref(), Some("22600:ABC234"));
        // openssl: echo -n "22600:ABC234" | openssl dgst -sha1 -hmac secret -binary | base64
        assert_eq!(
            relay.credential.as_deref(),
            Some("9UfkpEMe/UUHzAeQzzdreNj2vQY=")
        );
        settings.turn_secret.clear();
        assert_eq!(ice_servers_at(&settings, "ABC234", 1_000).len(), 1);
    }
}
