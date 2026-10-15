//! Reading values out of a response, as a definition's expressions describe.
//!
//! Shared by the indexer engine and the provider engine.

use crate::calendar::{DAY_MS, civil_from_ms, first_year, parse_date_ms};
use crate::definition::{Condition, Expression, Operation};
use crate::magnet::{display_name, info_hash_from_magnet, parse_size, size_from_magnet};
use crate::model::Kind;
use crate::template::Fill;
use anyhow::Context;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A compiled regular expression. Definitions are applied to every row of every response,
/// so each pattern is compiled once.
pub fn pattern(text: &str) -> anyhow::Result<Regex> {
    static COMPILED: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let mut compiled = COMPILED.get_or_init(Default::default).lock().unwrap();
    if let Some(regex) = compiled.get(text) {
        return Ok(regex.clone());
    }
    let regex = Regex::new(text).with_context(|| format!("bad pattern \"{text}\""))?;
    compiled.insert(text.to_string(), regex.clone());
    Ok(regex)
}

/// A row and, for a row nested in another, the one around it, and the kind being read.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    pub value: &'a Value,
    pub parent: Option<&'a Value>,
    pub kind: Kind,
}

/// What a condition may depend on besides the row.
pub struct Env<'a> {
    pub now_ms: i64,
    /// What is being asked for right now: `genre`, `year`, `sort:recent`.
    pub active: &'a [String],
}

/// The value at a dotted path: object keys, or list positions.
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    path.split('.').try_fold(value, |value, key| match value {
        Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        other => other.get(key),
    })
}

/// The values at a path, where `*` means every value of an object or list and a key met in a
/// list is read in each of its elements. A list found at the end is read element by element.
pub fn select<'a>(value: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut nodes = vec![value];
    if !path.is_empty() {
        for key in path.split('.') {
            nodes = nodes
                .into_iter()
                .flat_map(|node| -> Vec<&Value> {
                    match (key, node) {
                        ("*", Value::Object(map)) => map.values().collect(),
                        ("*", Value::Array(items)) => items.iter().collect(),
                        (key, Value::Array(items)) if key.parse::<usize>().is_err() => {
                            items.iter().filter_map(|item| item.get(key)).collect()
                        }
                        (key, node) => lookup(node, key).into_iter().collect(),
                    }
                })
                .collect();
        }
    }
    nodes
        .into_iter()
        .flat_map(|node| -> Vec<&Value> {
            match node {
                Value::Array(items) => items.iter().collect(),
                other => vec![other],
            }
        })
        .collect()
}

/// A scalar as text. Nothing for null, a list, an object or an empty text.
pub fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
    .filter(|text| !text.is_empty())
}

/// What a path reads in a row: `^.` starts from the row around this one, and `@kind` is the
/// kind being read.
fn read_all<'a>(path: &str, row: Row<'a>) -> Vec<&'a Value> {
    match path.strip_prefix("^.") {
        Some(outer) => row
            .parent
            .map_or_else(Vec::new, |parent| select(parent, outer)),
        None => select(row.value, path),
    }
}

fn read(path: &str, row: Row) -> Option<String> {
    if path == "@kind" {
        return Some(row.kind.as_str().to_string());
    }
    read_all(path, row).into_iter().find_map(scalar)
}

/// Turns an expression into text for one row, or nothing when the row has no such value.
pub fn evaluate(expression: &Expression, row: Row) -> Option<String> {
    let text = match expression {
        Expression::Path(path) => read(path, row)?,
        Expression::Many(parts) => parts.iter().find_map(|part| evaluate(part, row))?,
        Expression::Op(operation) => operate(operation, row)?,
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Turns an expression into the list of texts it stands for: a path to a list is all its
/// elements, a list of expressions is all of theirs, without repeats.
pub fn evaluate_list(expression: &Expression, row: Row) -> Vec<String> {
    let mut found: Vec<String> = match expression {
        Expression::Path(path) if path == "@kind" => vec![row.kind.as_str().to_string()],
        Expression::Path(path) => read_all(path, row).into_iter().filter_map(scalar).collect(),
        Expression::Many(parts) => parts
            .iter()
            .flat_map(|part| evaluate_list(part, row))
            .collect(),
        Expression::Op(Operation::Coalesce(parts)) => parts
            .iter()
            .map(|part| evaluate_list(part, row))
            .find(|list| !list.is_empty())
            .unwrap_or_default(),
        Expression::Op(Operation::ByKind(by_kind)) => by_kind
            .get(row.kind.as_str())
            .map(|part| evaluate_list(part, row))
            .unwrap_or_default(),
        other => evaluate(other, row).into_iter().collect(),
    };
    let mut seen = std::collections::HashSet::new();
    found.retain(|text| seen.insert(text.clone()));
    found
}

fn number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn integer(number: f64) -> String {
    (number.round() as i64).to_string()
}

fn operate(operation: &Operation, row: Row) -> Option<String> {
    match operation {
        Operation::Template(template) => {
            let hole = |name: &str| read(name, row);
            Fill {
                kind: row.kind,
                sort: None,
                hole: &hole,
            }
            .text(template)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        }
        Operation::Const(text) => Some(text.clone()),
        Operation::ByKind(by_kind) => evaluate(by_kind.get(row.kind.as_str())?, row),
        Operation::Regex {
            from,
            pattern: text,
        } => {
            let source = evaluate(from, row)?;
            let captures = pattern(text).ok()?.captures(&source)?;
            captures
                .get(1)
                .or_else(|| captures.get(0))
                .map(|found| found.as_str().to_string())
        }
        Operation::Line { from, skip, index } => {
            let source = evaluate(from, row)?;
            let skip = skip.as_deref().and_then(|text| pattern(text).ok());
            source
                .split('\n')
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter(|line| !skip.as_ref().is_some_and(|skip| skip.is_match(line)))
                .nth(*index)
                .map(str::to_string)
        }
        Operation::Coalesce(parts) => parts.iter().find_map(|part| evaluate(part, row)),
        Operation::Join(parts) => {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|part| evaluate(part, row))
                .collect();
            let kept: Vec<&str> = texts
                .iter()
                .enumerate()
                .filter(|(at, text)| {
                    !texts.iter().enumerate().any(|(other, longer)| {
                        other != *at
                            && longer.contains(text.as_str())
                            && (longer != *text || other < *at)
                    })
                })
                .map(|(_, text)| text.as_str())
                .collect();
            Some(kept.join(" "))
        }
        Operation::MagnetHash(from) => info_hash_from_magnet(&evaluate(from, row)?),
        Operation::MagnetName(from) => display_name(&evaluate(from, row)?),
        Operation::MagnetSize(from) => {
            size_from_magnet(&evaluate(from, row)?).map(|size| size.to_string())
        }
        Operation::Year(from) => first_year(&evaluate(from, row)?).map(|year| year.to_string()),
        Operation::Minutes(from) => {
            let text = evaluate(from, row)?;
            let start = text.find(|c: char| c.is_ascii_digit())?;
            Some(
                text[start..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect(),
            )
        }
        Operation::Scale { from, by } => Some(integer(number(&evaluate(from, row)?)? * by)),
        Operation::Add { from, n } => Some(integer(number(&evaluate(from, row)?)? + *n as f64)),
        Operation::Positive(from) => {
            let value = number(&evaluate(from, row)?)?;
            (value > 0.0).then(|| integer(value))
        }
        Operation::Map { from, table } => {
            let text = evaluate(from, row)?.to_lowercase();
            table
                .iter()
                .find(|(needle, _)| text.contains(needle.as_str()))
                .map(|(_, value)| value.clone())
        }
        Operation::PlainText(from) => Some(plain_text(&evaluate(from, row)?)),
    }
}

/// A size as a byte count or as text like "1.3 GB".
pub fn bytes(text: &str) -> Option<u64> {
    count(text).or_else(|| parse_size(text))
}

/// A number sent as a number, as text, or as a decimal.
pub fn count(text: &str) -> Option<u64> {
    let text = text.trim();
    text.parse::<u64>().ok().or_else(|| {
        text.parse::<f64>()
            .ok()
            .filter(|number| number.is_finite() && *number >= 0.0)
            .map(|number| number as u64)
    })
}

/// HTML as plain text: `<br>` is a line break, other tags go, entities are decoded and runs of
/// blank lines shrink to one.
fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_lowercase();
        if matches!(tag.as_str(), "br" | "br/" | "br /") {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);

    let mut out = out
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&mdash;", "\u{2014}")
        .replace("&amp;", "&");
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

/// Whether a row matches a condition: every test it names holds, and `when` (if given) is on.
pub fn matches(condition: &Condition, row: Row, env: &Env) -> bool {
    if let Some(when) = &condition.when
        && !env.active.contains(when)
    {
        return false;
    }
    let candidates = evaluate_list(&condition.path, row);
    let exists = match &condition.path {
        Expression::Path(path) => read_all(path, row).iter().any(|value| !value.is_null()),
        _ => !candidates.is_empty(),
    };
    let first = candidates.first().map(String::as_str);
    let later = |days: i64| env.now_ms + days * DAY_MS;

    // One result for each test the condition names.
    let mut results: Vec<bool> = Vec::new();
    if let Some(equals) = &condition.equals {
        results.push(candidates.iter().any(|text| text == equals));
    }
    if let Some(prefix) = &condition.starts_with {
        results.push(
            candidates
                .iter()
                .any(|text| text.starts_with(prefix.as_str())),
        );
    }
    if let Some(value) = &condition.differs {
        results.push(candidates.iter().all(|text| text != value));
    }
    if let Some(wanted) = condition.exists {
        results.push(exists == wanted);
    }
    if let Some(days) = &condition.after_today {
        let limit = later(days.grace_days);
        results.push(first.is_some_and(|text| match parse_date_ms(text) {
            Some(date) => date > limit,
            None => first_year(text).is_some_and(|year| year > civil_from_ms(limit).0),
        }));
    }
    if let Some(window) = &condition.outside_recent_years {
        let latest = civil_from_ms(later(window.grace_days)).0;
        results.push(
            first
                .and_then(first_year)
                .is_none_or(|year| year < latest - window.behind || year > latest),
        );
    }
    !results.is_empty() && results.into_iter().all(|holds| holds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn expression(value: Value) -> Expression {
        serde_json::from_value(value).unwrap()
    }

    fn row(value: &Value) -> Row<'_> {
        Row {
            value,
            parent: None,
            kind: Kind::Series,
        }
    }

    fn eval(expression_json: Value, value: &Value) -> Option<String> {
        evaluate(&expression(expression_json), row(value))
    }

    #[test]
    fn paths_reach_into_objects_and_lists() {
        let value = json!({ "a": { "b": [ { "c": 7 }, "x" ] } });
        assert_eq!(lookup(&value, "a.b.0.c"), Some(&json!(7)));
        assert_eq!(lookup(&value, "a.b.1"), Some(&json!("x")));
        assert_eq!(lookup(&value, "a.z"), None);
        assert_eq!(lookup(&value, "a.b.9"), None);
        assert_eq!(select(&json!({ "p": { "q": 1, "r": 2 } }), "p.*").len(), 2);
        // A key met in a list is read in each element.
        let schedule = json!([{ "_embedded": { "show": 1 } }, { "_embedded": { "show": 2 } }]);
        assert_eq!(select(&schedule, "_embedded.show"), [&json!(1), &json!(2)]);
        let edges = json!({ "edges": [{ "node": { "id": 1 } }, { "node": null }] });
        assert_eq!(select(&edges, "edges.*.node").len(), 2);
    }

    #[test]
    fn numbers_text_and_flags_are_read_as_text() {
        let value = json!({ "n": 5, "t": " hi ", "b": true, "e": "", "z": null, "o": {} });
        assert_eq!(eval(json!("n"), &value).as_deref(), Some("5"));
        assert_eq!(eval(json!("t"), &value).as_deref(), Some("hi"));
        assert_eq!(eval(json!("b"), &value).as_deref(), Some("true"));
        for empty in ["e", "z", "o", "missing"] {
            assert_eq!(eval(json!(empty), &value), None, "{empty}");
        }
        assert_eq!(eval(json!("@kind"), &value).as_deref(), Some("series"));
    }

    #[test]
    fn templates_read_the_row_and_the_row_around_it() {
        let outer = json!({ "title": "Dangal", "year": 2016 });
        let inner = json!({ "quality": "1080p" });
        let nested = Row {
            value: &inner,
            parent: Some(&outer),
            kind: Kind::Movie,
        };
        let template =
            expression(json!({ "template": "{^.title}{? ({^.year})?}{? {quality}?}{? {type}?}" }));
        assert_eq!(
            evaluate(&template, nested).as_deref(),
            Some("Dangal (2016) 1080p")
        );
        let missing = expression(json!({ "template": "{^.nothing} x" }));
        assert_eq!(evaluate(&missing, nested), None);
        let id = expression(json!({ "template": "{@kind}-{quality}" }));
        assert_eq!(evaluate(&id, nested).as_deref(), Some("movie-1080p"));
    }

    #[test]
    fn numbers_are_scaled_added_and_kept_only_when_positive() {
        let value = json!({ "rating": "9.5", "n": 6, "zero": 0, "runtime": "143 min", "year": "2008\u{2013}2013" });
        assert_eq!(
            eval(json!({ "scale": { "from": "rating", "by": 10 } }), &value).as_deref(),
            Some("95")
        );
        assert_eq!(
            eval(json!({ "scale": { "from": "rating", "by": 100 } }), &value).as_deref(),
            Some("950")
        );
        assert_eq!(
            eval(json!({ "add": { "from": "n", "n": -1 } }), &value).as_deref(),
            Some("5")
        );
        assert_eq!(
            eval(json!({ "positive": "n" }), &value).as_deref(),
            Some("6")
        );
        assert_eq!(eval(json!({ "positive": "zero" }), &value), None);
        assert_eq!(
            eval(
                json!({ "positive": { "add": { "from": "zero", "n": -1 } } }),
                &value
            ),
            None
        );
        assert_eq!(
            eval(json!({ "minutes": "runtime" }), &value).as_deref(),
            Some("143")
        );
        assert_eq!(
            eval(json!({ "year": "year" }), &value).as_deref(),
            Some("2008")
        );
        assert_eq!(eval(json!({ "minutes": "missing" }), &value), None);
    }

    #[test]
    fn a_vocabulary_is_mapped_by_what_the_text_contains() {
        let table = json!([
            ["ended", "FINISHED"],
            ["cancel", "CANCELLED"],
            ["running", "RELEASING"]
        ]);
        let map = |status: &str| {
            eval(
                json!({ "map": { "from": "status", "table": table } }),
                &json!({ "status": status }),
            )
        };
        assert_eq!(map("Ended").as_deref(), Some("FINISHED"));
        assert_eq!(map("Canceled/Ended").as_deref(), Some("FINISHED"));
        assert_eq!(
            map("Returning Series running").as_deref(),
            Some("RELEASING")
        );
        assert_eq!(map("To Be Determined"), None);
    }

    #[test]
    fn the_kind_picks_an_entry_and_a_constant_is_itself() {
        let by_kind =
            json!({ "by_kind": { "series": { "const": "TV" }, "movie": { "const": "MOVIE" } } });
        assert_eq!(eval(by_kind.clone(), &json!({})).as_deref(), Some("TV"));
        let movie = Row {
            value: &json!({}),
            parent: None,
            kind: Kind::Movie,
        };
        assert_eq!(
            evaluate(&expression(by_kind), movie).as_deref(),
            Some("MOVIE")
        );
        let anime = Row {
            value: &json!({}),
            parent: None,
            kind: Kind::Anime,
        };
        assert_eq!(
            evaluate(&expression(json!({ "by_kind": { "series": "x" } })), anime),
            None
        );
    }

    #[test]
    fn lists_gather_without_repeats_and_fall_back() {
        let value = json!({ "title": { "english": null, "romaji": "R", "native": "R" }, "genres": ["a", "b"], "genre": ["c"] });
        let names = expression(json!(["title.english", "title.romaji", "title.native"]));
        assert_eq!(evaluate_list(&names, row(&value)), ["R"]);
        let genres = expression(json!({ "coalesce": ["nothing", "genres"] }));
        assert_eq!(evaluate_list(&genres, row(&value)), ["a", "b"]);
        let studios = json!({ "nodes": [{ "name": "X" }, { "name": "Y" }] });
        assert_eq!(
            evaluate_list(&expression(json!("nodes.*.name")), row(&studios)),
            ["X", "Y"]
        );
    }

    #[test]
    fn html_is_reduced_to_plain_text() {
        let plain = |html: &str| eval(json!({ "plain_text": "t" }), &json!({ "t": html }));
        assert_eq!(plain("a<BR/>b<br />c").as_deref(), Some("a\nb\nc"));
        assert_eq!(plain("x\n\n\n\ny").as_deref(), Some("x\n\ny"));
        assert_eq!(
            plain("Line one<br>Line two &amp; <i>more</i>").as_deref(),
            Some("Line one\nLine two & more")
        );
        assert_eq!(
            plain("<p>Tom &amp; Jerry &quot;ok&quot;&nbsp;now</p>").as_deref(),
            Some("Tom & Jerry \"ok\" now")
        );
        assert_eq!(plain("  "), None);
    }

    fn condition(value: Value) -> Condition {
        serde_json::from_value(value).unwrap()
    }

    fn holds(condition_json: Value, value: &Value, now: &str, active: &[&str]) -> bool {
        let active: Vec<String> = active.iter().map(|text| text.to_string()).collect();
        let env = Env {
            now_ms: parse_date_ms(now).unwrap(),
            active: &active,
        };
        matches(&condition(condition_json), row(value), &env)
    }

    #[test]
    fn conditions_test_values_lists_and_presence() {
        let value = json!({ "name": "No results returned", "cats": [3000000, 5001000], "image": {}, "gone": null });
        let now = "2026-10-13";
        assert!(holds(
            json!({ "path": "name", "equals": "No results returned" }),
            &value,
            now,
            &[]
        ));
        assert!(!holds(
            json!({ "path": "name", "equals": "other" }),
            &value,
            now,
            &[]
        ));
        assert!(holds(
            json!({ "path": "cats", "starts_with": "5" }),
            &value,
            now,
            &[]
        ));
        assert!(holds(
            json!({ "path": "image", "exists": true }),
            &value,
            now,
            &[]
        ));
        assert!(holds(
            json!({ "path": "gone", "exists": false }),
            &value,
            now,
            &[]
        ));
        assert!(holds(
            json!({ "path": "absent", "exists": false }),
            &value,
            now,
            &[]
        ));
        // Without a test, nothing matches.
        assert!(!holds(json!({ "path": "name" }), &value, now, &[]));
        // A condition that names two tests needs both.
        assert!(!holds(
            json!({ "path": "name", "equals": "No results returned", "starts_with": "x" }),
            &value,
            now,
            &[]
        ));
    }

    #[test]
    fn a_title_not_out_yet_is_found_by_its_date_or_its_year() {
        let released =
            json!({ "path": { "coalesce": ["released", "releaseInfo"] }, "after_today": {} });
        let now = "2026-10-13";
        let check = |value: Value| holds(released.clone(), &value, now, &[]);
        assert!(!check(
            json!({ "released": "2026-06-06T00:00:00.000Z", "releaseInfo": "2026" })
        ));
        assert!(check(
            json!({ "released": "2026-12-02T00:00:00.000Z", "releaseInfo": "2026" })
        ));
        assert!(check(json!({ "releaseInfo": "2027" })));
        assert!(!check(json!({ "releaseInfo": "2025" })));
        assert!(!check(json!({})));
        // A few days of grace let a show about to premiere through.
        let grace = json!({ "path": "premiered", "after_today": { "grace_days": 30 } });
        assert!(!holds(
            grace.clone(),
            &json!({ "premiered": "2026-11-05" }),
            now,
            &[]
        ));
        assert!(holds(
            grace,
            &json!({ "premiered": "2026-11-20" }),
            now,
            &[]
        ));
    }

    #[test]
    fn a_recency_window_and_a_when_guard() {
        let window = json!({ "path": "year", "outside_recent_years": { "behind": 1, "grace_days": 30 }, "when": "sort:recent" });
        let now = "2026-10-13";
        let year = |year: i64| json!({ "year": year });
        assert!(!holds(window.clone(), &year(2026), now, &["sort:recent"]));
        assert!(!holds(window.clone(), &year(2025), now, &["sort:recent"]));
        assert!(holds(window.clone(), &year(2024), now, &["sort:recent"]));
        assert!(holds(window.clone(), &year(2027), now, &["sort:recent"]));
        assert!(holds(window.clone(), &json!({}), now, &["sort:recent"]));
        // Late in the year the grace days make next year count.
        assert!(!holds(
            window.clone(),
            &year(2027),
            "2026-12-25",
            &["sort:recent"]
        ));
        // Off unless the sort is the one named.
        assert!(!holds(window, &year(2024), now, &[]));
    }
}
