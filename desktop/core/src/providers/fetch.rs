//! Asking a provider: filling in a request, sending it and checking what comes back.

use crate::cache::{EMPTY_TTL, cached};
use crate::calendar::now_ms;
use crate::definition::{Check, Method, ProviderDef, ProviderRequest};
use crate::expression::{Env, Row, matches, scalar, select};
use crate::http::{client, read_capped};
use crate::model::{Kind, Sort};
use crate::sources;
use crate::template::Fill;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// A full day of a TV schedule is tens of megabytes of JSON; nothing legitimate is larger.
const MAX_BYTES: usize = 32 << 20;

/// What a request is about: the values its holes are filled with.
pub struct Asking<'a> {
    pub def: &'a ProviderDef,
    pub kind: Kind,
    pub sort: Option<Sort>,
    pub id: Option<&'a str>,
    pub term: Option<&'a str>,
    pub page: usize,
    /// How many rows the service is asked for.
    pub per_page: usize,
    pub skip: usize,
    pub genre: Option<&'a str>,
    pub year: Option<i32>,
}

impl<'a> Asking<'a> {
    pub fn new(def: &'a ProviderDef, kind: Kind) -> Asking<'a> {
        Asking {
            def,
            kind,
            sort: None,
            id: None,
            term: None,
            page: 1,
            per_page: 0,
            skip: 0,
            genre: None,
            year: None,
        }
    }

    fn hole(&self, name: &str) -> Option<String> {
        let def = self.def;
        match name {
            "base" => Some(def.base_url.clone()),
            "type" => def.types.get(&self.kind).cloned(),
            "sort" => self
                .sort
                .and_then(|sort| def.sorts.get(sort.as_str()))
                .or_else(|| def.sorts.get("default"))
                .cloned(),
            "id" => self.id.map(str::to_string),
            "term" => self
                .term
                .map(str::to_string)
                .filter(|term| !term.is_empty()),
            "page" => Some(self.page.to_string()),
            "per_page" => (self.per_page > 0).then(|| self.per_page.to_string()),
            "skip" => (self.skip > 0).then(|| self.skip.to_string()),
            "genre" => self.genre.map(str::to_string),
            "genre_lower" => self.genre.map(str::to_lowercase),
            "year" => self.year.map(|year| year.to_string()),
            _ => None,
        }
    }

    /// The request with its holes filled, or `None` when it needs something that is missing.
    pub fn prepare(&self, request: &ProviderRequest) -> Option<Prepared> {
        let hole = |name: &str| self.hole(name);
        let fill = Fill {
            kind: self.kind,
            sort: self.sort.map(Sort::as_str),
            hole: &hole,
        };
        let mut query = Vec::new();
        for (name, value) in &request.query {
            let (name, optional) = match name.strip_suffix('?') {
                Some(name) => (name, true),
                None => (name.as_str(), false),
            };
            match fill.value(value) {
                Some(Value::String(text)) => query.push((name.to_string(), text)),
                Some(other) => query.push((name.to_string(), other.to_string())),
                None if optional => {}
                None => return None,
            }
        }
        let json = match &request.json {
            Some(body) => Some(fill.value(body)?),
            None => None,
        };
        Some(Prepared {
            method: request.method,
            // The service's own address is not data to encode, only what goes after it.
            url: fill.address(&request.url.replace("{base}", &self.def.base_url))?,
            query,
            json,
        })
    }
}

/// A request ready to send.
pub struct Prepared {
    method: Method,
    url: String,
    query: Vec<(String, String)>,
    json: Option<Value>,
}

impl Prepared {
    #[cfg(test)]
    pub fn json_for_test(&self) -> Value {
        self.json.clone().unwrap_or(Value::Null)
    }

    #[cfg(test)]
    pub fn url_for_test(&self) -> String {
        self.url.clone()
    }

    #[cfg(test)]
    pub fn query_for_test(&self) -> Vec<(String, String)> {
        self.query.clone()
    }

    fn key(&self, provider: &str) -> String {
        // The definitions' version is part of it, so an update is not answered from an answer
        // read with the old one.
        format!(
            "{provider}:v{}:{:?}:{}:{:?}:{:?}",
            sources::current().version,
            self.method,
            self.url,
            self.query,
            self.json
        )
    }
}

/// The answer to a request, or `None` when the service has nothing there (a 404). It is shared
/// rather than copied, because some are tens of megabytes. It is reused for
/// `ttl`, and for a short while only when it holds no rows at `rows`.
pub async fn get(
    provider: &str,
    kind: Kind,
    request: Prepared,
    check: Option<&Check>,
    ttl: Duration,
    rows: &str,
) -> anyhow::Result<Option<Arc<Value>>> {
    let key = request.key(provider);
    let ttl_for = |answer: &Option<Arc<Value>>| match answer {
        Some(body) if select(body, rows).is_empty() => EMPTY_TTL,
        None => EMPTY_TTL,
        Some(_) => ttl,
    };
    cached(&key, ttl_for, || async {
        let builder = match request.method {
            Method::Get => client().get(&request.url),
            Method::Post => client().post(&request.url),
        };
        let mut builder = builder.query(&request.query);
        if let Some(body) = &request.json {
            builder = builder.json(body);
        }
        let response = builder.send().await?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bytes = read_capped(response, MAX_BYTES).await?;
        // A service that reports an error in its own words is read before the status is, so the
        // words are what is shown.
        let body: Option<Value> = serde_json::from_slice(&bytes).ok();
        if let (Some(body), Some(check)) = (&body, check) {
            failed(check, body, kind)?;
        }
        anyhow::ensure!(status.is_success(), "HTTP status {status}");
        body.map(|body| Some(Arc::new(body)))
            .ok_or_else(|| anyhow::anyhow!("not a JSON response"))
    })
    .await
}

/// An error if the answer says it failed.
fn failed(check: &Check, body: &Value, kind: Kind) -> anyhow::Result<()> {
    let row = Row {
        value: body,
        parent: None,
        kind,
    };
    let env = Env {
        now_ms: now_ms(),
        active: &[],
    };
    let reported = check
        .error
        .as_deref()
        .is_some_and(|path| select(body, path).iter().any(|value| !value.is_null()))
        || check.ok.as_ref().is_some_and(|ok| !matches(ok, row, &env));
    if !reported {
        return Ok(());
    }
    let message = check
        .message
        .as_deref()
        .and_then(|path| select(body, path).into_iter().find_map(scalar))
        .unwrap_or_else(|| "the service reported an error".to_string());
    anyhow::bail!("{message}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(value: Value) -> Check {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_answer_that_says_it_failed_is_an_error_with_its_own_words() {
        let by_error = check(json!({ "error": "errors", "message": "errors.0.message" }));
        let failure = json!({ "errors": [{ "message": "Too Many Requests" }] });
        let error = failed(&by_error, &failure, Kind::Anime).unwrap_err();
        assert_eq!(error.to_string(), "Too Many Requests");
        assert!(failed(&by_error, &json!({ "data": {} }), Kind::Anime).is_ok());
        assert!(failed(&by_error, &json!({ "errors": null }), Kind::Anime).is_ok());

        let by_status = check(
            json!({ "ok": { "path": "status", "equals": "ok" }, "message": "status_message" }),
        );
        assert!(failed(&by_status, &json!({ "status": "ok" }), Kind::Movie).is_ok());
        let error = failed(
            &by_status,
            &json!({ "status": "error", "status_message": "Query was empty" }),
            Kind::Movie,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Query was empty");
        let error = failed(&by_status, &json!({}), Kind::Movie).unwrap_err();
        assert_eq!(error.to_string(), "the service reported an error");
    }
}
