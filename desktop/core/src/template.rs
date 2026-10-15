//! Filling in the holes of a request template.
//!
//! A text has `{name}` holes and `{? ... ?}` groups that vanish when a hole inside is empty.
//! In a JSON template, `{"$kind": {...}}` and `{"$sort": {...}}` pick the entry for the kind or
//! the sort, `{"$number": "..."}` fills a text and reads it as a number, `{"$text": "..."}` is a
//! text left as it is (a GraphQL document is full of braces), and a key ending in `?` is left
//! out when its value is empty.

use crate::model::Kind;
use serde_json::{Map, Number, Value};

pub struct Fill<'a> {
    pub kind: Kind,
    /// The name of the sort asked for, if any.
    pub sort: Option<&'a str>,
    /// What a hole is filled with, or `None` when it has nothing.
    pub hole: &'a dyn Fn(&str) -> Option<String>,
}

impl Fill<'_> {
    pub fn text(&self, template: &str) -> Option<String> {
        self.fill(template, false)
    }

    /// Like `text`, with each hole percent-encoded, for a place in an address.
    pub fn address(&self, template: &str) -> Option<String> {
        self.fill(template, true)
    }

    fn fill(&self, template: &str, encode: bool) -> Option<String> {
        let mut text = String::new();
        // The group being read, and whether every hole in it had a value.
        let mut group: Option<(String, bool)> = None;
        let mut rest = template;
        while !rest.is_empty() {
            let (output, next) = if let Some(after) = rest.strip_prefix("{?") {
                group = Some((String::new(), true));
                (None, after)
            } else if let Some(after) = rest.strip_prefix("?}") {
                if let Some((inside, true)) = group.take() {
                    text.push_str(&inside);
                }
                (None, after)
            } else if let Some(after) = rest.strip_prefix('{') {
                let close = after.find('}')?;
                let value = (self.hole)(&after[..close]).map(|value| {
                    if encode {
                        percent_encode(&value)
                    } else {
                        value
                    }
                });
                (Some(value), &after[close + 1..])
            } else {
                let width = rest.chars().next().map_or(1, char::len_utf8);
                let (literal, next) = rest.split_at(width);
                match &mut group {
                    Some((inside, _)) => inside.push_str(literal),
                    None => text.push_str(literal),
                }
                (None, next)
            };
            if let Some(value) = output {
                match (&mut group, value) {
                    (Some((inside, _)), Some(value)) => inside.push_str(&value),
                    (Some((_, complete)), None) => *complete = false,
                    (None, Some(value)) => text.push_str(&value),
                    // A hole outside any group that has nothing empties the whole value.
                    (None, None) => return None,
                }
            }
            rest = next;
        }
        Some(text)
    }

    /// A template tree filled in, or `None` when something it needs is missing.
    pub fn value(&self, template: &Value) -> Option<Value> {
        match template {
            Value::String(text) => self.text(text).map(Value::String),
            Value::Array(items) => items.iter().map(|item| self.value(item)).collect(),
            Value::Object(map) => self.object(map),
            other => Some(other.clone()),
        }
    }

    fn object(&self, map: &Map<String, Value>) -> Option<Value> {
        if map.len() == 1 {
            match map.iter().next()? {
                (key, Value::Object(choices)) if key == "$kind" => {
                    return self.value(choices.get(self.kind.as_str())?);
                }
                (key, Value::Object(choices)) if key == "$sort" => {
                    let chosen = self
                        .sort
                        .and_then(|sort| choices.get(sort))
                        .or_else(|| choices.get("default"))?;
                    return self.value(chosen);
                }
                (key, Value::String(text)) if key == "$text" => {
                    return Some(Value::String(text.clone()));
                }
                (key, Value::String(text)) if key == "$number" => {
                    let filled = self.text(text)?;
                    return filled
                        .parse::<i64>()
                        .map(Number::from)
                        .ok()
                        .or_else(|| Number::from_f64(filled.parse().ok()?))
                        .map(Value::Number);
                }
                _ => {}
            }
        }
        let mut filled = Map::new();
        for (key, item) in map {
            match (key.strip_suffix('?'), self.value(item)) {
                (_, Some(value)) => {
                    filled.insert(key.trim_end_matches('?').to_string(), value);
                }
                (Some(_), None) => {}
                (None, None) => return None,
            }
        }
        Some(Value::Object(filled))
    }
}

/// Percent-encodes like JavaScript's `encodeURIComponent`.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fill<'a>(hole: &'a dyn Fn(&str) -> Option<String>, sort: Option<&'a str>) -> Fill<'a> {
        Fill {
            kind: Kind::Series,
            sort,
            hole,
        }
    }

    fn holes(name: &str) -> Option<String> {
        match name {
            "term" => Some("dil se".into()),
            "page" => Some("2".into()),
            "year" => None,
            _ => None,
        }
    }

    #[test]
    fn holes_are_filled_and_empty_groups_vanish() {
        let fill = fill(&holes, None);
        assert_eq!(fill.text("q={term}").as_deref(), Some("q=dil se"));
        assert_eq!(fill.text("a{? y={year}?}b").as_deref(), Some("ab"));
        assert_eq!(fill.text("a{? p={page}?}b").as_deref(), Some("a p=2b"));
        // A hole with nothing, outside a group, empties the value.
        assert_eq!(fill.text("a{year}b"), None);
        assert_eq!(fill.text("ünï {term}").as_deref(), Some("ünï dil se"));
    }

    #[test]
    fn an_address_encodes_what_goes_into_it() {
        let fill = fill(&holes, None);
        assert_eq!(
            fill.address("/search={term}.json").as_deref(),
            Some("/search=dil%20se.json")
        );
    }

    #[test]
    fn selectors_numbers_literals_and_optional_keys() {
        let fill = fill(&holes, Some("recent"));
        let template = json!({
            "doc": { "$text": "query { Page { id } }" },
            "page": { "$number": "{page}" },
            "year?": "{year}",
            "term": "{term}",
            "by_kind": { "$kind": { "series": "s", "movie": "m" } },
            "by_sort": { "$sort": { "recent": "r", "default": "d" } },
            "only_default?": { "$sort": { "popularity": "p" } },
        });
        assert_eq!(
            fill.value(&template).unwrap(),
            json!({
                "doc": "query { Page { id } }", "page": 2, "term": "dil se",
                "by_kind": "s", "by_sort": "r"
            })
        );
        // A required key that has nothing takes the whole object with it.
        assert_eq!(fill.value(&json!({ "year": "{year}" })), None);
        // No entry for the sort falls back to `default`.
        let plain = self::fill(&holes, None);
        assert_eq!(
            plain.value(&json!({ "$sort": { "recent": "r", "default": "d" } })),
            Some(json!("d"))
        );
        assert_eq!(plain.value(&json!({ "$kind": { "movie": "m" } })), None);
    }
}
