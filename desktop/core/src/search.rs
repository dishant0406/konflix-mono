//! Search across every catalogue.
//!
//! A title is addressed by a provider-specific id, so each provider searches one kind.
//! Searching a single kind would silently hide the rest: "Breaking Bad" is a series,
//! "Dangal" is a film and "Bocchi" is an anime, and one search box must find all three.

use crate::model::{Kind, Media};
use crate::providers;

/// Searches each kind at once and merges the answers into one list of at most `limit`.
/// `Err` only when every kind failed, so a dead catalogue does not hide the others.
pub async fn search(term: &str, kinds: &[Kind], limit: usize) -> anyhow::Result<Vec<Media>> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let pages = futures::future::join_all(kinds.iter().map(|&kind| search_kind(kind, term))).await;
    Ok(combine(answered(pages)?, term, limit))
}

/// One catalogue's matches, for a caller that shows each as it arrives rather than waiting for
/// the slowest.
pub async fn search_kind(kind: Kind, term: &str) -> anyhow::Result<Vec<Media>> {
    Ok(providers::search(kind, term.trim()).await?.results)
}

/// The answers of the catalogues that came back, as one list of at most `limit`.
pub fn combine(lists: Vec<Vec<Media>>, term: &str, limit: usize) -> Vec<Media> {
    prefer_title_matches(interleave(lists, limit), term.trim())
}

/// The result lists of the kinds that answered. `Err` when none did.
fn answered(pages: Vec<anyhow::Result<Vec<Media>>>) -> anyhow::Result<Vec<Vec<Media>>> {
    let mut lists = Vec::new();
    let mut last_error = None;
    for page in pages {
        match page {
            Ok(page) => lists.push(page),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) if lists.is_empty() => Err(error),
        _ => Ok(lists),
    }
}

/// Interleaves per-catalogue lists round-robin, so the first page shows a mix.
/// Concatenating would fill the page from whichever catalogue is listed first.
///
/// Ordering within a catalogue is the provider's own relevance. There is no score the
/// providers share, so nothing here ranks one catalogue above another.
pub fn interleave(lists: Vec<Vec<Media>>, limit: usize) -> Vec<Media> {
    let lists: Vec<Vec<Media>> = lists.into_iter().filter(|list| !list.is_empty()).collect();
    let mut merged = Vec::new();
    let mut index = 0;
    while merged.len() < limit {
        let mut added = false;
        for list in &lists {
            if let Some(item) = list.get(index)
                && merged.len() < limit
            {
                merged.push(item.clone());
                added = true;
            }
        }
        // Every list is exhausted, so there is nothing left to add.
        if !added {
            break;
        }
        index += 1;
    }
    merged
}

/// Letters and digits only, so punctuation and spacing do not decide a match.
fn normalise(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Moves results whose title starts with the term to the front, keeping the rest in
/// order. Without it the round-robin puts an approximate match first: TVMaze's own
/// search is fuzzy, so "dangal" returned "D.N.Angel" before the film "Dangal".
pub fn prefer_title_matches(results: Vec<Media>, term: &str) -> Vec<Media> {
    let wanted = normalise(term);
    if wanted.is_empty() {
        return results;
    }
    let (matching, rest): (Vec<Media>, Vec<Media>) = results.into_iter().partition(|media| {
        let title = normalise(&media.title);
        // A title the term covers entirely, allowing one trailing character of slack:
        // "dangal" still matches a title stored as "Dangal (2016)", while the four-letter
        // "DANG!" must not match it, as it would under a plain prefix test.
        let covered =
            !title.is_empty() && wanted.len() <= title.len() + 1 && wanted.starts_with(&title);
        title.starts_with(&wanted) || covered
    });
    matching.into_iter().chain(rest).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str) -> Media {
        Media {
            id: id.into(),
            kind: Kind::Movie,
            title: title.into(),
            title_variants: vec![],
            poster: None,
            banner: None,
            description: None,
            format: None,
            year: None,
            episodes: None,
            runtime_minutes: None,
            status: None,
            score: None,
            popularity: 0,
            genres: vec![],
            is_adult: false,
        }
    }

    fn ids(list: &[Media]) -> Vec<&str> {
        list.iter().map(|media| media.id.as_str()).collect()
    }

    #[test]
    fn a_kind_that_failed_is_skipped_when_another_answered() {
        let lists = answered(vec![Err(anyhow::anyhow!("down")), Ok(vec![item("a", "")])]);
        assert_eq!(lists.unwrap().len(), 1);
    }

    #[test]
    fn every_kind_failing_is_an_error() {
        assert!(answered(vec![Err(anyhow::anyhow!("a")), Err(anyhow::anyhow!("b"))]).is_err());
    }

    #[test]
    fn results_from_two_catalogues_are_interleaved_not_concatenated() {
        let merged = interleave(
            vec![
                vec![item("series-1", ""), item("series-2", "")],
                vec![item("movie-1", ""), item("movie-2", "")],
            ],
            10,
        );
        assert_eq!(ids(&merged), ["series-1", "movie-1", "series-2", "movie-2"]);
    }

    #[test]
    fn a_catalogue_that_returns_nothing_consumes_none_of_the_page() {
        let merged = interleave(
            vec![vec![item("series-1", ""), item("series-2", "")], vec![]],
            10,
        );
        assert_eq!(ids(&merged), ["series-1", "series-2"]);
    }

    #[test]
    fn the_merge_stops_at_the_requested_size() {
        let merged = interleave(
            vec![
                vec![item("a", ""), item("b", ""), item("c", ""), item("d", "")],
                vec![item("e", ""), item("f", ""), item("g", ""), item("h", "")],
            ],
            3,
        );
        assert_eq!(ids(&merged), ["a", "e", "b"]);
    }

    #[test]
    fn a_longer_catalogue_keeps_contributing_after_a_shorter_one_runs_out() {
        let merged = interleave(
            vec![
                vec![item("series-1", "")],
                vec![
                    item("movie-1", ""),
                    item("movie-2", ""),
                    item("movie-3", ""),
                ],
            ],
            10,
        );
        assert_eq!(ids(&merged), ["series-1", "movie-1", "movie-2", "movie-3"]);
    }

    #[test]
    fn a_title_starting_with_the_term_moves_ahead_of_a_fuzzy_match() {
        let results = vec![
            item("a", "D.N.Angel"),
            item("b", "Dangal"),
            item("c", "Dangal (2016)"),
        ];
        assert_eq!(
            ids(&prefer_title_matches(results, "dangal")),
            ["b", "c", "a"]
        );
    }

    #[test]
    fn a_short_title_is_not_matched_by_a_longer_term() {
        let results = vec![item("a", "DANG!"), item("b", "Dangal")];
        assert_eq!(ids(&prefer_title_matches(results, "dangal")), ["b", "a"]);
    }

    #[test]
    fn an_empty_term_changes_nothing() {
        let results = vec![item("a", "x"), item("b", "y")];
        assert_eq!(ids(&prefer_title_matches(results, "  ")), ["a", "b"]);
    }
}
