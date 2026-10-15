//! A page of titles, or a search, from a provider's definition.

use super::card;
use super::fetch::{Asking, get};
use crate::calendar::now_ms;
use crate::definition::{Compare, ListDef, Next, Order, Paging, ProviderDef, SearchDef, Style};
use crate::expression::{Env, Row, count, evaluate, evaluate_list, matches, scalar, select};
use crate::model::{Filters, Kind, Media, Page, Sort};
use futures::future::join_all;
use serde_json::Value;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::time::Duration;

/// How the rows of a page are got and cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The service pages. `widened` is a wider page filtered here, with no next page.
    Page { widened: bool },
    /// The service pages by skip. `windowed` is a run of pages from the start, cut here.
    Skip { windowed: bool },
    /// One answer, cut here.
    Pool,
}

/// What a page is asked for, settled: the page number and size within the limits.
struct Wanted<'a> {
    filters: &'a Filters,
    page: usize,
    per_page: usize,
    /// What is being asked for, for conditions that depend on it.
    active: Vec<String>,
    mode: Mode,
}

impl<'a> Wanted<'a> {
    fn new(list: &ListDef, filters: &'a Filters) -> Wanted<'a> {
        let paging: &Paging = &list.paging;
        let mut active = Vec::new();
        if filters
            .genre
            .as_deref()
            .is_some_and(|genre| !genre.is_empty())
        {
            active.push("genre".to_string());
        }
        if filters.year.is_some() {
            active.push("year".to_string());
        }
        if let Some(sort) = filters.sort {
            active.push(format!("sort:{}", sort.as_str()));
        }
        let any = |when: &[String]| when.iter().any(|token| active.contains(token));
        let mode = match &paging.style {
            Style::Page { widen, .. } => Mode::Page {
                widened: widen.as_ref().is_some_and(|widen| any(&widen.when)),
            },
            Style::Skip { window, .. } => Mode::Skip {
                windowed: any(&window.when),
            },
            Style::Pool => Mode::Pool,
        };
        let per_page = match filters.per_page as usize {
            0 => paging.per_page,
            asked => asked.clamp(1, paging.max),
        };
        Wanted {
            filters,
            page: (filters.page as usize).max(1),
            per_page,
            active,
            mode,
        }
    }

    fn genre(&self) -> Option<&str> {
        self.filters
            .genre
            .as_deref()
            .filter(|genre| !genre.is_empty())
    }
}

pub async fn list(def: &ProviderDef, kind: Kind, filters: &Filters) -> anyhow::Result<Page> {
    let Some(list) = &def.list else {
        return Ok(Page::default());
    };
    let wanted = Wanted::new(list, filters);

    // The requests: one, or for a skipping service several that cover a run of rows.
    let (scans, offset, step, rows_asked) = match (&list.paging.style, wanted.mode) {
        (Style::Skip { size, window }, Mode::Skip { windowed }) if windowed => {
            (window.pages, 0, *size, wanted.per_page)
        }
        (Style::Skip { size, .. }, _) => (
            wanted.per_page.div_ceil(*size) + 1,
            (wanted.page - 1) * wanted.per_page,
            *size,
            wanted.per_page,
        ),
        (
            Style::Page {
                widen: Some(widen), ..
            },
            Mode::Page { widened: true },
        ) => (1, 0, 0, widen.per_page),
        _ => (1, 0, 0, wanted.per_page),
    };
    let ttl = Duration::from_secs(list.ttl_secs);
    let answers = join_all((0..scans).map(|index| {
        let mut asking = Asking::new(def, kind);
        asking.sort = filters.sort;
        asking.page = wanted.page;
        asking.per_page = rows_asked;
        asking.skip = offset + index * step;
        asking.genre = wanted.genre();
        asking.year = filters.year;
        let request = asking.prepare(&list.request);
        async move {
            let request =
                request.ok_or_else(|| anyhow::anyhow!("the request cannot be filled in"))?;
            get(
                &def.id,
                kind,
                request,
                list.check.as_ref(),
                ttl,
                &list.rows,
                true,
            )
            .await
        }
    }))
    .await;
    // A run of pages with one failing is still a run: it is an error only when none came.
    let mut bodies = Vec::new();
    let mut failure = None;
    for answer in answers {
        match answer {
            Ok(Some(body)) => bodies.push(body),
            Ok(None) => {}
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    if let (true, Some(error)) = (bodies.is_empty(), failure) {
        return Err(error);
    }
    Ok(shape(def, list, kind, &wanted, &bodies, now_ms()))
}

/// The page that answers make, once they are in.
fn shape<B: Borrow<Value>>(
    def: &ProviderDef,
    list: &ListDef,
    kind: Kind,
    wanted: &Wanted,
    bodies: &[B],
    now_ms: i64,
) -> Page {
    let env = Env {
        now_ms,
        active: &wanted.active,
    };
    let row = |value| Row {
        value,
        parent: None,
        kind,
    };
    let raw: Vec<Vec<&Value>> = bodies
        .iter()
        .map(|body| select(body.borrow(), &list.rows))
        .collect();

    // Left out: rows twice over, rows the list drops, rows another filter or the viewer's does not
    // want.
    let mut seen = HashSet::new();
    let mut rows: Vec<&Value> = raw
        .iter()
        .flatten()
        .copied()
        .filter(|value| match &list.dedupe {
            Some(key) => evaluate(key, row(value)).is_some_and(|key| seen.insert(key)),
            None => true,
        })
        .filter(|value| !list.drop.iter().any(|c| matches(c, row(value), &env)))
        .filter(|value| {
            let genre = wanted.genre().map(str::to_lowercase);
            match (&list.genre, genre) {
                (Some(path), Some(genre)) => evaluate_list(path, row(value))
                    .iter()
                    .any(|name| name.to_lowercase() == genre),
                _ => true,
            }
        })
        .filter(|value| match (&list.year, wanted.filters.year) {
            (Some(path), Some(year)) => evaluate(path, row(value))
                .and_then(|text| text.parse::<i32>().ok())
                .is_some_and(|found| found == year),
            _ => true,
        })
        .collect();

    // Rows cut here are ordered here, because the service did not.
    let local = matches!(wanted.mode, Mode::Pool | Mode::Skip { windowed: true });
    if local {
        let sort = wanted.filters.sort.map(Sort::as_str);
        let order = sort
            .and_then(|name| list.order.get(name))
            .or_else(|| list.order.get("default"));
        if let Some(order) = order {
            sort_rows(&mut rows, order, kind);
        }
    }

    let card = |value| card::media(&def.card, row(value));
    let start = (wanted.page - 1) * wanted.per_page;
    match wanted.mode {
        Mode::Page { widened } => Page {
            results: rows.into_iter().filter_map(card).collect(),
            has_next: !widened
                && match &list.paging.style {
                    Style::Page { next, .. } => bodies.first().is_some_and(|body| {
                        more(next, body.borrow(), wanted.page, wanted.per_page)
                    }),
                    _ => false,
                },
        },
        Mode::Skip { windowed: false } => {
            let size = match &list.paging.style {
                Style::Skip { size, .. } => *size,
                _ => usize::MAX,
            };
            // Whether the catalogue goes on is decided by the answer as it came, before anything
            // was left out of it.
            let has_next = raw.last().is_some_and(|last| last.len() >= size);
            Page {
                results: rows
                    .into_iter()
                    .filter_map(card)
                    .take(wanted.per_page)
                    .collect(),
                has_next,
            }
        }
        Mode::Skip { windowed: true } | Mode::Pool => Page {
            has_next: start + wanted.per_page < rows.len(),
            results: rows
                .into_iter()
                .skip(start)
                .take(wanted.per_page)
                .filter_map(card)
                .collect(),
        },
    }
}

/// Whether the service has more after this page.
fn more(next: &Next, body: &Value, page: usize, per_page: usize) -> bool {
    match next {
        Next::Flag(path) => select(body, path)
            .into_iter()
            .find_map(scalar)
            .is_some_and(|flag| flag == "true"),
        Next::Total(path) => select(body, path)
            .into_iter()
            .find_map(scalar)
            .and_then(|total| count(&total))
            .is_some_and(|total| ((page * per_page) as u64) < total),
    }
}

/// One value of a sort key. A number or a text, or nothing when the row has none.
#[derive(Debug, Clone, PartialEq)]
enum Key {
    Number(f64),
    Text(String),
}

fn sort_rows(rows: &mut Vec<&Value>, order: &[Order], kind: Kind) {
    let key = |value: &Value, by: &Order| -> Option<Key> {
        let row = Row {
            value,
            parent: None,
            kind,
        };
        let text = evaluate(&by.by, row)?;
        match by.compare {
            Compare::Number => text.parse().ok().map(Key::Number),
            Compare::Caseless => Some(Key::Text(text.to_lowercase())),
            Compare::Text => Some(Key::Text(text)),
        }
    };
    let mut keyed: Vec<(Vec<Option<Key>>, &Value)> = rows
        .iter()
        .map(|value| (order.iter().map(|by| key(value, by)).collect(), *value))
        .collect();
    keyed.sort_by(|(a, _), (b, _)| {
        for (index, by) in order.iter().enumerate() {
            let ordering = match (&a[index], &b[index]) {
                (None, None) => Ordering::Equal,
                (Some(_), None) if by.missing_last => Ordering::Less,
                (None, Some(_)) if by.missing_last => Ordering::Greater,
                (a, b) => {
                    let nothing = match by.compare {
                        Compare::Number => Key::Number(0.0),
                        _ => Key::Text(String::new()),
                    };
                    let ordering = match (
                        a.as_ref().unwrap_or(&nothing),
                        b.as_ref().unwrap_or(&nothing),
                    ) {
                        (Key::Number(a), Key::Number(b)) => {
                            a.partial_cmp(b).unwrap_or(Ordering::Equal)
                        }
                        (Key::Text(a), Key::Text(b)) => a.cmp(b),
                        _ => Ordering::Equal,
                    };
                    if by.descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                }
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });
    *rows = keyed.into_iter().map(|(_, value)| value).collect();
}

pub async fn search(def: &ProviderDef, kind: Kind, term: &str) -> anyhow::Result<Page> {
    let term = term.trim();
    let Some(search) = &def.search else {
        return Ok(Page::default());
    };
    if term.is_empty() {
        return Ok(Page::default());
    }
    let per_page = search.per_page.unwrap_or(0);
    let mut asking = Asking::new(def, kind);
    asking.term = Some(term);
    asking.per_page = per_page;
    let request = asking
        .prepare(&search.request)
        .ok_or_else(|| anyhow::anyhow!("the request cannot be filled in"))?;
    let ttl = Duration::from_secs(search.ttl_secs);
    let body = get(
        &def.id,
        kind,
        request,
        search.check.as_ref(),
        ttl,
        &search.rows,
        false,
    )
    .await?;
    Ok(body
        .map(|body| shape_search(def, search, kind, &body, now_ms()))
        .unwrap_or_default())
}

fn shape_search(
    def: &ProviderDef,
    search: &SearchDef,
    kind: Kind,
    body: &Value,
    now_ms: i64,
) -> Page {
    let env = Env {
        now_ms,
        active: &[],
    };
    let row = |value| Row {
        value,
        parent: None,
        kind,
    };
    let results: Vec<Media> = select(body, &search.rows)
        .into_iter()
        .filter(|value| !search.drop.iter().any(|c| matches(c, row(value), &env)))
        .filter_map(|value| card::media(&def.card, row(value)))
        .collect();
    let has_next = search
        .next
        .as_ref()
        .is_some_and(|next| more(next, body, 1, search.per_page.unwrap_or(0)));
    Page { results, has_next }
}

#[cfg(test)]
pub(super) fn shape_for_test(
    def: &ProviderDef,
    kind: Kind,
    filters: &Filters,
    bodies: &[Value],
    now_ms: i64,
) -> Page {
    let list = def.list.as_ref().expect("the provider lists");
    shape(def, list, kind, &Wanted::new(list, filters), bodies, now_ms)
}

#[cfg(test)]
pub(super) fn shape_search_for_test(def: &ProviderDef, kind: Kind, body: &Value) -> Page {
    let search = def.search.as_ref().expect("the provider searches");
    shape_search(def, search, kind, body, 0)
}
