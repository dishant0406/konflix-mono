//! Metadata providers.
//!
//! A provider answers "what titles exist" and "what is this one". Each kind has an ordered chain:
//! the next provider answers when the first returns nothing or fails, so an outage degrades a row
//! instead of emptying it.
//!
//! Which providers there are, where they are and how their answers are read is data
//! (`definition::ProviderDef`); this module is the one engine that follows those definitions.

mod card;
mod detail;
mod disk;
mod fetch;
mod list;
#[cfg(test)]
mod read_tests;

use crate::definition::ProviderDef;
use crate::model::{Detail, Filters, Kind, Page};
use crate::sources::{self, Sources};

/// Keeps catalogue answers in `dir` between runs, so a start shows what the last one saw while
/// the new answers come in.
pub fn keep_answers_in(dir: std::path::PathBuf) {
    disk::start(dir);
}

/// The providers that answer for a kind, in the order the definitions give.
fn chain(sources: &Sources, kind: Kind) -> Vec<&ProviderDef> {
    sources
        .chain(kind)
        .iter()
        .filter_map(|id| sources.provider(id))
        .collect()
}

/// Whether every request of a definition can be filled in with just what that request has: a
/// misspelt hole, or one that is empty when it matters (the id of a list, the genre nobody asked
/// for), would otherwise only look like a provider that never answers.
pub fn check(def: &ProviderDef) -> anyhow::Result<()> {
    for &kind in &def.kinds {
        let ensure_fills = |what: &str, asking: &fetch::Asking, request| -> anyhow::Result<()> {
            anyhow::ensure!(
                asking.prepare(request).is_some(),
                "the {what} request cannot be filled in for {}",
                kind.as_str()
            );
            Ok(())
        };
        let mut about = fetch::Asking::new(def, kind);
        about.id = Some("1");
        ensure_fills("detail", &about, &def.detail.request)?;
        for list in [&def.detail.related, &def.detail.recommended]
            .into_iter()
            .flatten()
        {
            if let Some(request) = &list.request {
                ensure_fills("related", &about, request)?;
            }
        }
        if let Some(list) = &def.list {
            let mut page = fetch::Asking::new(def, kind);
            page.per_page = 24;
            ensure_fills("list", &page, &list.request)?;
            page.skip = 48;
            ensure_fills("list", &page, &list.request)?;
        }
        if let Some(search) = &def.search {
            let mut asking = fetch::Asking::new(def, kind);
            asking.term = Some("sample");
            asking.per_page = search.per_page.unwrap_or(0);
            ensure_fills("search", &asking, &search.request)?;
        }
    }
    Ok(())
}

/// Walks a chain: the first non-empty page wins. `Ok` with an empty page means at least one
/// provider answered and had nothing; `Err` means every provider failed, which the caller must
/// tell apart from "nothing matched" (an offline app is not an empty catalogue).
async fn first_page<P, F, Fut>(what: &str, chain: &[P], mut ask: F) -> anyhow::Result<Page>
where
    P: Copy + std::fmt::Display,
    F: FnMut(P) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Page>>,
{
    let mut answered = false;
    let mut last_error = None;
    for &provider in chain {
        match ask(provider).await {
            Ok(page) if !page.results.is_empty() => return Ok(page),
            Ok(_) => answered = true,
            Err(error) => {
                tracing::warn!("[providers] {provider}.{what} failed: {error:#}");
                last_error = Some(error);
            }
        }
    }
    match last_error {
        Some(error) if !answered => Err(error),
        _ => Ok(Page::default()),
    }
}

/// A provider as a link of a chain, named by its id in the log.
#[derive(Clone, Copy)]
struct Link<'a>(&'a ProviderDef);

impl std::fmt::Display for Link<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.id)
    }
}

fn links(sources: &Sources, kind: Kind) -> Vec<Link<'_>> {
    chain(sources, kind).into_iter().map(Link).collect()
}

/// One browse row. `Err` only when every provider in the kind's chain failed.
pub async fn list(kind: Kind, filters: &Filters) -> anyhow::Result<Page> {
    let sources = sources::current();
    first_page("list", &links(&sources, kind), |link| {
        list::list(link.0, kind, filters)
    })
    .await
}

/// A search within one kind, with the same fallback rules as `list`.
pub async fn search(kind: Kind, term: &str) -> anyhow::Result<Page> {
    let sources = sources::current();
    first_page("search", &links(&sources, kind), |link| {
        list::search(link.0, kind, term)
    })
    .await
}

/// A title in full. `id` is a title key such as `series-tt0903747`; an episode id is accepted too
/// and reduced to its title.
pub async fn detail(id: &str) -> Option<Detail> {
    let parsed = crate::media_id::parse(id)?;
    let sources = sources::current();
    for provider in links(&sources, parsed.kind) {
        match detail::detail(provider.0, parsed.kind, &parsed.provider_id).await {
            Ok(Some(detail)) if !detail.media.id.is_empty() => return Some(detail),
            Ok(_) => {}
            Err(error) => tracing::warn!("[providers] {provider}.detail failed: {error:#}"),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Media;

    fn page(count: usize) -> Page {
        let media = Media {
            id: "movie-1".into(),
            kind: Kind::Movie,
            title: String::new(),
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
        };
        Page {
            results: vec![media; count],
            ..Page::default()
        }
    }

    /// Answers each provider from a script, in chain order.
    async fn walk(script: Vec<anyhow::Result<Page>>) -> anyhow::Result<Page> {
        let names = ["first", "second"];
        let mut script = script.into_iter();
        first_page("test", &names[..script.len()], |_| {
            std::future::ready(script.next().unwrap())
        })
        .await
    }

    #[tokio::test]
    async fn the_first_non_empty_page_wins() {
        let found = walk(vec![Err(anyhow::anyhow!("down")), Ok(page(2))]).await;
        assert_eq!(found.unwrap().results.len(), 2);
    }

    #[tokio::test]
    async fn an_empty_answer_is_ok_even_when_another_provider_failed() {
        let found = walk(vec![Err(anyhow::anyhow!("down")), Ok(page(0))]).await;
        assert!(found.unwrap().results.is_empty());
    }

    #[tokio::test]
    async fn every_provider_failing_is_an_error() {
        let found = walk(vec![Err(anyhow::anyhow!("a")), Err(anyhow::anyhow!("b"))]).await;
        assert!(found.is_err());
    }
}
