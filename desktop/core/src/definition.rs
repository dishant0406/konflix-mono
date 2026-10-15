//! The shapes a definition in `sources.json` is made of.
//!
//! Nothing here runs: these are plain data that the indexer engine (`indexers/`) and the
//! provider engine (`providers/`) read. Reading a value out of a response is described with an
//! `Expression`, which both engines share (`expression.rs`).

use crate::model::Kind;
use anyhow::ensure;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------
// Reading a value
// ---------------------------------------------------------------------------------------------

/// How a value is read out of a row. A bare string is a dotted path, a list is several
/// expressions whose values are gathered, and an object is one of the operations.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Expression {
    Path(String),
    Many(Vec<Expression>),
    Op(Operation),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Text with `{path}` holes (a `^.` prefix reads the row around this one, `@kind` is the kind
    /// searched), and `{? ... ?}` groups that vanish when a hole inside is empty:
    /// `"{^.title}{? ({^.year})?}"`. A hole outside a group that is empty empties the value.
    Template(String),
    /// A fixed text.
    Const(String),
    /// The expression for the kind being searched, when there is one.
    ByKind(BTreeMap<String, Expression>),
    /// The first capture group of a regular expression.
    Regex {
        from: Box<Expression>,
        pattern: String,
    },
    /// One line of a multi-line text, skipping lines that match `skip`.
    Line {
        from: Box<Expression>,
        #[serde(default)]
        skip: Option<String>,
        index: usize,
    },
    /// The first of these that has a value.
    Coalesce(Vec<Expression>),
    /// The parts, space separated, leaving out a part that another contains.
    Join(Vec<Expression>),
    /// What a magnet link carries: its info hash, its display name, its size in bytes.
    MagnetHash(Box<Expression>),
    MagnetName(Box<Expression>),
    MagnetSize(Box<Expression>),
    /// The first four digits in a text, as a year ("2008–2013" is 2008).
    Year(Box<Expression>),
    /// The first number in a text ("143 min" is 143).
    Minutes(Box<Expression>),
    /// A number times `by`, rounded: a rating out of 10 as a score out of 100.
    Scale {
        from: Box<Expression>,
        by: f64,
    },
    /// A number plus `n`.
    Add {
        from: Box<Expression>,
        n: i64,
    },
    /// The number when it is above zero, otherwise nothing.
    Positive(Box<Expression>),
    /// The value for the first needle that the lowercased text contains.
    Map {
        from: Box<Expression>,
        table: Vec<(String, String)>,
    },
    /// HTML as plain text: line breaks kept, tags removed, entities decoded.
    PlainText(Box<Expression>),
}

impl Expression {
    /// Rejects what can be known to be wrong without a response: a bad pattern.
    pub fn check(&self) -> anyhow::Result<()> {
        match self {
            Expression::Path(_) => {}
            Expression::Many(parts) => {
                for part in parts {
                    part.check()?;
                }
            }
            Expression::Op(operation) => operation.check()?,
        }
        Ok(())
    }
}

impl Operation {
    fn check(&self) -> anyhow::Result<()> {
        match self {
            Operation::Template(_) | Operation::Const(_) => {}
            Operation::ByKind(by_kind) => {
                for expression in by_kind.values() {
                    expression.check()?;
                }
            }
            Operation::Regex { from, pattern } => {
                crate::expression::pattern(pattern)?;
                from.check()?;
            }
            Operation::Line { from, skip, .. } => {
                if let Some(skip) = skip {
                    crate::expression::pattern(skip)?;
                }
                from.check()?;
            }
            Operation::Coalesce(parts) | Operation::Join(parts) => {
                for part in parts {
                    part.check()?;
                }
            }
            Operation::MagnetHash(from)
            | Operation::MagnetName(from)
            | Operation::MagnetSize(from)
            | Operation::Year(from)
            | Operation::Minutes(from)
            | Operation::Positive(from)
            | Operation::PlainText(from) => from.check()?,
            Operation::Scale { from, .. }
            | Operation::Add { from, .. }
            | Operation::Map { from, .. } => from.check()?,
        }
        Ok(())
    }
}

/// A test on a row. All the tests a condition names must hold for it to match.
#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    /// What is tested. When it reaches a list, any element may match.
    pub path: Expression,
    #[serde(default)]
    pub equals: Option<String>,
    #[serde(default)]
    pub starts_with: Option<String>,
    /// Nothing at the path is this (and a path that leads to nothing counts).
    #[serde(default)]
    pub differs: Option<String>,
    /// Whether the path leads to something (not null) at all.
    #[serde(default)]
    pub exists: Option<bool>,
    /// The date or year found is later than today plus a few days: not out yet.
    #[serde(default)]
    pub after_today: Option<Days>,
    /// The year found is not this year or the `behind` before it (today plus a few days
    /// decides what "this year" is).
    #[serde(default)]
    pub outside_recent_years: Option<RecentYears>,
    /// The condition counts only while this holds: `genre` or `year` is being asked for, or
    /// `sort:recent` (or another sort's name) is the sort.
    #[serde(default)]
    pub when: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Days {
    #[serde(default)]
    pub grace_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecentYears {
    pub behind: i64,
    #[serde(default)]
    pub grace_days: i64,
}

impl Condition {
    pub fn check(&self) -> anyhow::Result<()> {
        if let Some(when) = &self.when {
            check_when(std::slice::from_ref(when))?;
        }
        let days = [
            self.after_today.as_ref().map(|days| days.grace_days),
            self.outside_recent_years
                .as_ref()
                .map(|years| years.grace_days),
            self.outside_recent_years.as_ref().map(|years| years.behind),
        ];
        ensure!(
            days.into_iter().flatten().all(|n| (0..=3650).contains(&n)),
            "a number of days or years is out of range"
        );
        self.path.check()
    }
}

// ---------------------------------------------------------------------------------------------
// Indexers
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fanout {
    /// Once per search term.
    PerTerm,
    /// Once per distinct title. The indexer answers with every episode of a show, or has no
    /// episode syntax, so asking it per episode-number form would fetch the same response
    /// several times over.
    PerTitle,
    /// Exactly once: the term plays no part in the request, an IMDb id does.
    Once,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerDef {
    /// The name the viewer sees in Settings and the `source` of each release.
    pub id: String,
    pub name: String,
    pub kinds: Vec<Kind>,
    #[serde(default = "per_term")]
    pub fanout: Fanout,
    /// Mirrors of one service, tried in order. One that fails is skipped for
    /// `cooldown_secs`, so a blocked mirror is not asked again for every search term.
    pub hosts: Vec<String>,
    #[serde(default = "a_minute")]
    pub cooldown_secs: u64,
    pub request: RequestDef,
    pub response: ResponseDef,
}

fn per_term() -> Fanout {
    Fanout::PerTerm
}

fn a_minute() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestDef {
    #[serde(default)]
    pub method: Method,
    /// Appended to the host. A string, or `{"$kind": {...}}` to differ per kind.
    #[serde(default = "no_text")]
    pub path: Value,
    #[serde(default)]
    pub query: BTreeMap<String, Value>,
    /// A JSON body, for POST.
    #[serde(default)]
    pub json: Option<Value>,
}

fn no_text() -> Value {
    Value::String(String::new())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Rss,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseDef {
    pub format: Format,
    /// Where the rows are in a JSON body: dotted keys, with `*` for "every value". An
    /// array found there is read element by element. Empty means the body itself. An RSS
    /// feed has no such path: its rows are its items.
    #[serde(default)]
    pub rows: String,
    /// Rows nested inside each row (the torrents of a film). Their fields can read the
    /// outer row with a `^.` prefix.
    #[serde(default)]
    pub children: Option<String>,
    pub fields: Fields,
    /// A row matching any of these is dropped.
    #[serde(default)]
    pub drop: Vec<Condition>,
    /// The row's title must contain the whole searched title. For an index with no
    /// category to filter on, so unrelated rows are not returned.
    #[serde(default)]
    pub must_contain_term: bool,
    /// One row per info hash.
    #[serde(default)]
    pub dedupe: Option<Dedupe>,
}

impl ResponseDef {
    fn check(&self) -> anyhow::Result<()> {
        let fields = &self.fields;
        let optional = [&fields.magnet, &fields.size, &fields.seeders];
        for expression in [&fields.title, &fields.hash]
            .into_iter()
            .chain(optional.into_iter().flatten())
        {
            expression.check()?;
        }
        for condition in &self.drop {
            condition.check()?;
        }
        if let Some(Dedupe { avoid }) = &self.dedupe {
            crate::expression::pattern(avoid)?;
        }
        Ok(())
    }
}

/// The release fields, each read out of a row.
#[derive(Debug, Clone, Deserialize)]
pub struct Fields {
    pub title: Expression,
    /// A 40 character hex or 32 character base32 info hash.
    pub hash: Expression,
    /// Used when it is a magnet link for this very hash. Otherwise a magnet is built from the
    /// hash and the trackers, which most indexers need because they send none.
    #[serde(default)]
    pub magnet: Option<Expression>,
    #[serde(default)]
    pub size: Option<Expression>,
    #[serde(default)]
    pub seeders: Option<Expression>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dedupe {
    /// When several rows share a hash, the first whose title does not match this is kept
    /// (or the first of all if every one matches). Keeps a bonus clip from standing for
    /// the season pack it was listed with.
    pub avoid: String,
}

impl IndexerDef {
    pub fn check(&self) -> anyhow::Result<()> {
        ensure!(!self.id.is_empty(), "an indexer has no id");
        ensure!(!self.kinds.is_empty(), "serves no kind");
        ensure!(
            (1..=8).contains(&self.hosts.len()),
            "needs between 1 and 8 hosts"
        );
        ensure!(self.cooldown_secs <= 3600, "rests longer than an hour");
        for host in &self.hosts {
            check_url(host)?;
        }
        self.response.check()?;
        crate::indexers::check(self)
    }
}

// ---------------------------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------------------------

/// One metadata provider: where it is, how to ask it for a page of titles, a search and one
/// title, and how to read the answers.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderDef {
    pub id: String,
    /// The kinds of title it has.
    pub kinds: Vec<Kind>,
    /// The `{base}` of every request, so a service that moves is one change.
    pub base_url: String,
    /// What the service calls each kind: the `{type}` of a request.
    #[serde(default)]
    pub types: BTreeMap<Kind, String>,
    /// What the service calls each sort: the `{sort}` of a request. `default` covers a sort
    /// not named, and a request with no sort.
    #[serde(default)]
    pub sorts: BTreeMap<String, String>,
    /// How a title is read, wherever the service gives one: a page, a search, a title in full,
    /// the titles related to it.
    pub card: MediaMap,
    #[serde(default)]
    pub list: Option<ListDef>,
    #[serde(default)]
    pub search: Option<SearchDef>,
    pub detail: DetailDef,
}

/// A request to a provider. `url` is the whole address, and its holes are percent-encoded.
/// The holes: `{base}`, `{type}`, `{sort}`, `{id}`, `{term}`, `{page}`, `{per_page}`, `{skip}`
/// (nothing when 0), `{genre}` and `{year}` (nothing when not asked for).
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderRequest {
    #[serde(default)]
    pub method: Method,
    pub url: String,
    #[serde(default)]
    pub query: BTreeMap<String, Value>,
    /// A JSON body. In it `{"$text": "..."}` is a text left as it is, `{"$number": "{page}"}` a
    /// number, `{"$sort": {...}}` picks by the sort, and a key ending in `?` is left out when
    /// its value is empty.
    #[serde(default)]
    pub json: Option<Value>,
}

/// How to tell that an answer is an error even though it arrived.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// An answer that has anything at this path failed.
    #[serde(default)]
    pub error: Option<String>,
    /// An answer must satisfy this to have worked.
    #[serde(default)]
    pub ok: Option<Condition>,
    /// Where the answer says why it failed.
    #[serde(default)]
    pub message: Option<String>,
}

impl Check {
    fn check(&self) -> anyhow::Result<()> {
        if let Some(ok) = &self.ok {
            ok.check()?;
        }
        Ok(())
    }
}

/// How a title is read out of a row. The id is the provider's own: the title's key is made from
/// the kind and this.
#[derive(Debug, Clone, Deserialize)]
pub struct MediaMap {
    pub id: Expression,
    pub title: Expression,
    /// Other names, gathered without repeats. By default the title alone.
    #[serde(default)]
    pub title_variants: Option<Expression>,
    #[serde(default)]
    pub poster: Option<Expression>,
    #[serde(default)]
    pub banner: Option<Expression>,
    #[serde(default)]
    pub description: Option<Expression>,
    #[serde(default)]
    pub format: Option<Expression>,
    #[serde(default)]
    pub status: Option<Expression>,
    #[serde(default)]
    pub year: Option<Expression>,
    #[serde(default)]
    pub episodes: Option<Expression>,
    #[serde(default)]
    pub runtime_minutes: Option<Expression>,
    /// Out of 100.
    #[serde(default)]
    pub score: Option<Expression>,
    #[serde(default)]
    pub popularity: Option<Expression>,
    #[serde(default)]
    pub genres: Option<Expression>,
    #[serde(default)]
    pub adult: Option<Expression>,
}

impl MediaMap {
    fn check(&self) -> anyhow::Result<()> {
        let required = [&self.id, &self.title];
        let optional = [
            &self.title_variants,
            &self.poster,
            &self.banner,
            &self.description,
            &self.format,
            &self.status,
            &self.year,
            &self.episodes,
            &self.runtime_minutes,
            &self.score,
            &self.popularity,
            &self.genres,
            &self.adult,
        ];
        for expression in required.into_iter().chain(optional.into_iter().flatten()) {
            expression.check()?;
        }
        Ok(())
    }
}

/// A page of titles.
#[derive(Debug, Clone, Deserialize)]
pub struct ListDef {
    pub request: ProviderRequest,
    /// Where the rows are in the answer, as for an indexer.
    pub rows: String,
    #[serde(default)]
    pub check: Option<Check>,
    /// How long an answer is reused.
    #[serde(default = "ten_minutes")]
    pub ttl_secs: u64,
    /// Rows with the same value here are one title: the first counts.
    #[serde(default)]
    pub dedupe: Option<Expression>,
    /// A row matching any of these is left out.
    #[serde(default)]
    pub drop: Vec<Condition>,
    pub paging: Paging,
    /// How a row names its genres and its year, for the rows that are filtered here because the
    /// service cannot (see `paging`).
    #[serde(default)]
    pub genre: Option<Expression>,
    #[serde(default)]
    pub year: Option<Expression>,
    /// How rows that are sorted here are ordered, by the sort's name, and `default`.
    #[serde(default)]
    pub order: BTreeMap<String, Vec<Order>>,
}

fn ten_minutes() -> u64 {
    10 * 60
}

#[derive(Debug, Clone, Deserialize)]
pub struct Paging {
    /// A page of this many titles when none is asked for, and never more than `max`.
    #[serde(default = "default_per_page")]
    pub per_page: usize,
    #[serde(default = "max_per_page")]
    pub max: usize,
    #[serde(flatten)]
    pub style: Style,
}

fn default_per_page() -> usize {
    24
}

fn max_per_page() -> usize {
    50
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "style", rename_all = "kebab-case")]
pub enum Style {
    /// The service pages: it is asked for page `{page}` of `{per_page}`. When one of the `when`
    /// of `widen` holds, it is asked for `per_page` rows of the page instead and the rows are
    /// filtered here, with no next page.
    Page {
        next: Next,
        #[serde(default)]
        widen: Option<Widen>,
    },
    /// The service pages by `{skip}`, `size` rows at a time. When one of the `when` of `window`
    /// holds (the service cannot filter or order that way) `pages` of them are read from the start
    /// and the page is cut from those here.
    Skip { size: usize, window: Window },
    /// The service has no paging: it is asked once and the page is cut here.
    Pool,
}

/// How to tell whether the service has another page after this one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Next {
    /// A true value at this path.
    Flag(String),
    /// A count at this path that is more than the rows up to this page.
    Total(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Widen {
    pub when: Vec<String>,
    pub per_page: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Window {
    pub when: Vec<String>,
    pub pages: usize,
}

/// One key of an ordering: rows are compared by the first key, then by the next.
#[derive(Debug, Clone, Deserialize)]
pub struct Order {
    pub by: Expression,
    #[serde(default)]
    pub compare: Compare,
    #[serde(default)]
    pub descending: bool,
    /// Rows with nothing here go after the rest, instead of counting as 0 (or as empty text).
    #[serde(default)]
    pub missing_last: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Compare {
    #[default]
    Text,
    /// Text without regard to case.
    Caseless,
    Number,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchDef {
    pub request: ProviderRequest,
    pub rows: String,
    #[serde(default)]
    pub check: Option<Check>,
    #[serde(default = "ten_minutes")]
    pub ttl_secs: u64,
    #[serde(default)]
    pub drop: Vec<Condition>,
    /// How many rows the search asks for: its `{per_page}`.
    #[serde(default)]
    pub per_page: Option<usize>,
    /// Whether there is more than the page that came. Without it there is not.
    #[serde(default)]
    pub next: Option<Next>,
}

/// One title in full.
#[derive(Debug, Clone, Deserialize)]
pub struct DetailDef {
    pub request: ProviderRequest,
    /// The ids the service has, as a pattern. Anything else is not asked for: the title is not
    /// there.
    #[serde(default)]
    pub id_pattern: Option<String>,
    #[serde(default)]
    pub check: Option<Check>,
    /// Where the title is in the answer. The paths below start there.
    #[serde(default)]
    pub root: String,
    /// A title matching any of these does not exist (a service that answers a placeholder).
    #[serde(default)]
    pub missing: Vec<Condition>,
    /// Where the synopsis is: the card has none, because a page of titles does not carry it.
    #[serde(default)]
    pub description: Option<Expression>,
    /// The title's number of episodes is the number listed.
    #[serde(default)]
    pub count_episodes: bool,
    #[serde(default)]
    pub imdb_id: Option<Expression>,
    #[serde(default)]
    pub studios: Option<Expression>,
    #[serde(default)]
    pub trailer: Option<TrailerMap>,
    #[serde(default)]
    pub episodes: Option<EpisodesMap>,
    #[serde(default)]
    pub cast: Option<CastMap>,
    #[serde(default)]
    pub related: Option<MediaList>,
    #[serde(default)]
    pub recommended: Option<MediaList>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrailerMap {
    pub site: Expression,
    pub id: Expression,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodesMap {
    pub rows: String,
    #[serde(default)]
    pub drop: Vec<Condition>,
    /// The episode's whole media id.
    pub id: Expression,
    pub season: Expression,
    pub number: Expression,
    #[serde(default)]
    pub name: Option<Expression>,
    #[serde(default)]
    pub airdate: Option<Expression>,
}

/// The cast, from the first of the sources that has anyone.
#[derive(Debug, Clone, Deserialize)]
pub struct CastMap {
    pub sources: Vec<CastSource>,
    #[serde(default = "twelve")]
    pub limit: usize,
}

fn twelve() -> usize {
    12
}

#[derive(Debug, Clone, Deserialize)]
pub struct CastSource {
    pub rows: String,
    pub name: Expression,
    #[serde(default)]
    pub role: Option<Expression>,
    #[serde(default)]
    pub image: Option<Expression>,
}

/// Titles related to this one: found in the title's own answer, or in an answer of their own.
#[derive(Debug, Clone, Deserialize)]
pub struct MediaList {
    /// Asked separately, and a failure leaves the list empty. Without it the rows are in the
    /// title's own answer, below its root.
    #[serde(default)]
    pub request: Option<ProviderRequest>,
    #[serde(default)]
    pub check: Option<Check>,
    pub rows: String,
    #[serde(default)]
    pub drop: Vec<Condition>,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl ProviderDef {
    pub fn check(&self) -> anyhow::Result<()> {
        ensure!(!self.id.is_empty(), "a provider has no id");
        ensure!(!self.kinds.is_empty(), "serves no kind");
        check_url(&self.base_url)?;
        // The base is put into a template, so it cannot hold template syntax.
        ensure!(
            !self.base_url.contains(['{', '}']),
            "the base address has braces in it"
        );

        // Every request goes to the service's own address: what is searched for or asked after
        // is only ever put after it.
        let mut urls: Vec<&str> = vec![&self.detail.request.url];
        urls.extend(self.list.iter().map(|list| list.request.url.as_str()));
        urls.extend(self.search.iter().map(|search| search.request.url.as_str()));
        for list in [&self.detail.related, &self.detail.recommended]
            .into_iter()
            .flatten()
        {
            urls.extend(list.request.iter().map(|request| request.url.as_str()));
        }
        for url in urls {
            ensure!(
                url == "{base}" || url.starts_with("{base}/"),
                "\"{url}\" does not start at the base address"
            );
        }

        let detail = &self.detail;
        if let Some(pattern) = &detail.id_pattern {
            crate::expression::pattern(pattern)?;
        }
        for condition in &detail.missing {
            condition.check()?;
        }
        if let Some(check) = &detail.check {
            check.check()?;
        }
        self.card.check()?;
        let optional = [&detail.imdb_id, &detail.studios, &detail.description];
        for expression in optional.into_iter().flatten() {
            expression.check()?;
        }
        if let Some(trailer) = &detail.trailer {
            trailer.site.check()?;
            trailer.id.check()?;
        }
        if let Some(episodes) = &detail.episodes {
            for expression in [&episodes.id, &episodes.season, &episodes.number] {
                expression.check()?;
            }
            for expression in [&episodes.name, &episodes.airdate].into_iter().flatten() {
                expression.check()?;
            }
            for condition in &episodes.drop {
                condition.check()?;
            }
        }
        if let Some(cast) = &detail.cast {
            for source in &cast.sources {
                source.name.check()?;
                for expression in [&source.role, &source.image].into_iter().flatten() {
                    expression.check()?;
                }
            }
        }
        for list in [&detail.related, &detail.recommended].into_iter().flatten() {
            for condition in &list.drop {
                condition.check()?;
            }
            if let Some(check) = &list.check {
                check.check()?;
            }
        }

        if let Some(list) = &self.list {
            ensure!(
                list.ttl_secs <= MAX_TTL_SECS,
                "an answer is reused for too long"
            );
            for expression in [&list.dedupe, &list.genre, &list.year]
                .into_iter()
                .flatten()
            {
                expression.check()?;
            }
            for condition in &list.drop {
                condition.check()?;
            }
            for order in list.order.values().flatten() {
                order.by.check()?;
            }
            if let Some(check) = &list.check {
                check.check()?;
            }
            let paging = &list.paging;
            ensure!(
                (1..=200).contains(&paging.max) && (1..=paging.max).contains(&paging.per_page),
                "the page sizes are out of range"
            );
            match &paging.style {
                Style::Skip { size, window } => {
                    // A page is read as this many requests at once, so they are few.
                    ensure!(
                        *size > 0 && paging.max.div_ceil(*size) <= 4,
                        "pages of too few rows for the largest page"
                    );
                    ensure!((1..=4).contains(&window.pages), "a window of 1 to 4 pages");
                    check_when(&window.when)?;
                }
                Style::Page { widen, .. } => {
                    if let Some(widen) = widen {
                        ensure!(
                            (1..=200).contains(&widen.per_page),
                            "the widened page is out of range"
                        );
                        check_when(&widen.when)?;
                    }
                }
                Style::Pool => {}
            }
        }
        if let Some(search) = &self.search {
            ensure!(
                search.ttl_secs <= MAX_TTL_SECS,
                "an answer is reused for too long"
            );
            ensure!(
                search.per_page.is_none_or(|rows| (1..=200).contains(&rows)),
                "the search page is out of range"
            );
            // Whether there is more is counted in pages of this size.
            ensure!(
                search.next.is_none() || search.per_page.is_some(),
                "a search that says whether there is more says how many it asks for"
            );
            for condition in &search.drop {
                condition.check()?;
            }
            if let Some(check) = &search.check {
                check.check()?;
            }
        }
        crate::providers::check(self)
    }
}

/// The longest an answer is reused: a day.
const MAX_TTL_SECS: u64 = 24 * 60 * 60;

/// A condition's `when` is `genre`, `year` or `sort:<name>`; anything else would never hold.
fn check_when(tokens: &[String]) -> anyhow::Result<()> {
    for token in tokens {
        ensure!(
            token == "genre" || token == "year" || token.starts_with("sort:"),
            "\"{token}\" is not something that can be asked for"
        );
    }
    Ok(())
}

pub fn check_url(url: &str) -> anyhow::Result<()> {
    ensure!(
        url.starts_with("https://") || url.starts_with("http://"),
        "\"{url}\" is not an http(s) address"
    );
    Ok(())
}
