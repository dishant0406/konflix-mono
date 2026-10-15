//! Reading titles, episodes and people out of an answer, as the definition describes.

use crate::definition::{CastMap, DetailDef, EpisodesMap, MediaList, MediaMap};
use crate::expression::{Env, Row, evaluate, evaluate_list, matches, select};
use crate::media_id::title_key;
use crate::model::{Detail, Episode, Kind, Media, Person, Trailer};
use serde_json::Value;

/// One title as a card, or `None` for a row with no id of the service's own.
pub fn media(map: &MediaMap, row: Row) -> Option<Media> {
    let text = |expression: &Option<_>| expression.as_ref().and_then(|e| evaluate(e, row));
    let whole = |expression: &Option<_>| text(expression).and_then(|t| t.parse::<f64>().ok());
    let id = evaluate(&map.id, row)?;
    let title = evaluate(&map.title, row).unwrap_or_default();
    let title_variants = match &map.title_variants {
        Some(expression) => evaluate_list(expression, row),
        None if title.is_empty() => Vec::new(),
        None => vec![title.clone()],
    };
    Some(Media {
        id: title_key(row.kind, &id),
        kind: row.kind,
        title,
        title_variants,
        poster: text(&map.poster),
        banner: text(&map.banner),
        description: text(&map.description),
        format: text(&map.format),
        year: whole(&map.year).map(|year| year as i32),
        episodes: whole(&map.episodes).map(|count| count as u32),
        runtime_minutes: whole(&map.runtime_minutes).map(|minutes| minutes as u32),
        status: text(&map.status),
        score: whole(&map.score).map(|score| score as u32),
        popularity: whole(&map.popularity).map_or(0, |popularity| popularity as i64),
        genres: map
            .genres
            .as_ref()
            .map(|e| evaluate_list(e, row))
            .unwrap_or_default(),
        is_adult: text(&map.adult).is_some_and(|flag| flag == "true"),
    })
}

/// The titles of one related list, read from the title's own answer or from the one it was asked
/// for separately.
fn related_list(
    card: &MediaMap,
    list: Option<&MediaList>,
    kind: Kind,
    title: &Value,
    separate: Option<&Value>,
    env: &Env,
) -> Vec<Media> {
    let Some(list) = list else {
        return Vec::new();
    };
    let (source, parent) = match &list.request {
        Some(_) => (separate, None),
        None => (Some(title), Some(title)),
    };
    source
        .map(|source| cards(card, select(source, &list.rows), parent, kind, list, env))
        .unwrap_or_default()
}

/// The cards in a list of rows, leaving out the rows the list drops.
fn cards(
    map: &MediaMap,
    rows: Vec<&Value>,
    parent: Option<&Value>,
    kind: Kind,
    list: &MediaList,
    env: &Env,
) -> Vec<Media> {
    rows.into_iter()
        .map(|value| Row {
            value,
            parent,
            kind,
        })
        .filter(|row| {
            !list
                .drop
                .iter()
                .any(|condition| matches(condition, *row, env))
        })
        .filter_map(|row| media(map, row))
        .take(list.limit.unwrap_or(usize::MAX))
        .collect()
}

/// One title in full. `title` is the title's own object in the answer. A list that is asked for
/// separately reads its rows from `separate`, the answer it came in.
pub fn detail(
    card: &MediaMap,
    def: &DetailDef,
    kind: Kind,
    title: &Value,
    separate: [Option<&Value>; 2],
    env: &Env,
) -> Option<Detail> {
    let row = Row {
        value: title,
        parent: None,
        kind,
    };
    let mut media = media(card, row)?;
    media.description = def.description.as_ref().and_then(|e| evaluate(e, row));

    let episodes = def
        .episodes
        .as_ref()
        .map(|map| episodes(map, title, kind, env))
        .unwrap_or_default();
    // A count that is the length of the list is more exact than the one the card carries.
    if def.count_episodes && !episodes.is_empty() {
        media.episodes = Some(episodes.len() as u32);
    }

    let related = related_list(card, def.related.as_ref(), kind, title, separate[0], env);
    let recommended = related_list(
        card,
        def.recommended.as_ref(),
        kind,
        title,
        separate[1],
        env,
    );

    Some(Detail {
        imdb_id: def.imdb_id.as_ref().and_then(|e| evaluate(e, row)),
        media,
        studios: def
            .studios
            .as_ref()
            .map(|e| evaluate_list(e, row))
            .unwrap_or_default(),
        trailer: def.trailer.as_ref().and_then(|trailer| {
            Some(Trailer {
                site: evaluate(&trailer.site, row)?,
                id: evaluate(&trailer.id, row)?,
            })
        }),
        episodes,
        cast: def
            .cast
            .as_ref()
            .map(|map| cast(map, title, kind))
            .unwrap_or_default(),
        related,
        recommended,
    })
}

fn episodes(map: &EpisodesMap, title: &Value, kind: Kind, env: &Env) -> Vec<Episode> {
    select(title, &map.rows)
        .into_iter()
        .map(|value| Row {
            value,
            parent: Some(title),
            kind,
        })
        .filter(|row| {
            !map.drop
                .iter()
                .any(|condition| matches(condition, *row, env))
        })
        .filter_map(|row| {
            let number = evaluate(&map.number, row)?.parse::<f64>().ok()? as u32;
            let season = evaluate(&map.season, row)?.parse::<f64>().ok()? as u32;
            Some(Episode {
                id: evaluate(&map.id, row)?,
                season: Some(season),
                number,
                name: map.name.as_ref().and_then(|e| evaluate(e, row)),
                airdate: map.airdate.as_ref().and_then(|e| evaluate(e, row)),
            })
        })
        .collect()
}

/// The cast from the first source that has anyone in it.
fn cast(map: &CastMap, title: &Value, kind: Kind) -> Vec<Person> {
    for source in &map.sources {
        let people: Vec<Person> = select(title, &source.rows)
            .into_iter()
            .map(|value| Row {
                value,
                parent: Some(title),
                kind,
            })
            .filter_map(|row| {
                Some(Person {
                    name: evaluate(&source.name, row)?,
                    role: source.role.as_ref().and_then(|e| evaluate(e, row)),
                    image: source.image.as_ref().and_then(|e| evaluate(e, row)),
                })
            })
            .take(map.limit)
            .collect();
        if !people.is_empty() {
            return people;
        }
    }
    Vec::new()
}
