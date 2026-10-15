# Source definitions

`sources.json` says where Akidev looks for titles and for releases: the indexers, the
metadata providers, their addresses, and the public trackers. The app has one copy built in
and fetches a newer one in the background, so an indexer can be added, removed or moved
without a new build. Nothing in it is code: a definition only says which requests to make and
which fields to read.

## Publishing a change

1. Edit `sources.json` and raise `version` (every publish must be higher than the live one).
2. `mkdir -p .secrets` once. `cargo test -p akidev-core` checks the built-in copy parses and that each indexer reads a
   sample response correctly. `cargo run -p akidev-core --bin akidev-sources -- check sources/sources.json`
   validates a copy on its own.
3. `./scripts/publish-sources.sh` signs it and uploads it. Running apps pick it up within six
   hours, and anyone who opens the app after that gets it at start.

The app uses a downloaded copy only if its signature is valid, its `version` is higher than
the one in use and it passes the same validation as the built-in one (limits, addresses, regular
expressions, requests that can be filled in, chains that name providers serving the kind).
Otherwise it keeps what it has. Validation cannot know whether a mirror is alive or a GraphQL
selection is right, so test a change against the live services before publishing it; a mistake
that gets through is undone by publishing a corrected copy with a higher version.

### One-time setup

- Create an R2 bucket and give it the custom domain `sources.akidev.live` (done with the cf CLI:
  `cf r2 buckets create-by-name akidev-sources`, then `cf r2 buckets domains custom create
  akidev-sources --domain sources.akidev.live --zone-id <zone> --min-tls 1.2 --enabled`).
- `cargo run -p akidev-core --bin akidev-sources -- keygen .secrets/sources.key` makes the
  signing key and prints its public half. The public half is `PUBLIC_KEY` in
  `core/src/sources.rs`. The private file is git-ignored: keep a copy in a password manager.
  Losing it means shipping a build with a new public key.

## Format

`schema` is the version of this format (an app ignores a document with a higher one),
`version` is the version of the content. `trackers` are added to every magnet. `chains` list
the providers that answer for each kind, in order: the next answers when the first has
nothing or fails.

### Indexers

```json
{ "id": "knaben", "name": "Knaben", "kinds": ["anime", "series", "movie"],
  "fanout": "per-term", "hosts": ["https://api.knaben.org/v1"], "cooldown_secs": 60,
  "request":  { "method": "POST", "path": "", "query": {}, "json": {} },
  "response": { "format": "json", "rows": "hits", "fields": { "title": "title", "hash": "hash" } } }
```

- `fanout`: `per-term` asks once per search term, `per-title` once per distinct title (for an
  index that answers with every episode, or has no episode syntax), `once` a single time (the
  term plays no part, as with an IMDb id).
- `hosts` are mirrors of one service, tried in order. A host that fails rests for
  `cooldown_secs`.
- In `path`, `query` and `json`, `{term}`, `{imdb}` (`tt0903747`), `{imdb_digits}` (`0903747`), `{imdb_or_term}` (the id when the search has
  one, otherwise the term),
  `{season}` and `{episode}` are filled in. A request that needs a value the search lacks (an
  IMDb id) is not made. `{"$kind": {"anime": ..., "movie": ...}}` is replaced by the entry for
  the kind being searched.
- `format` is `json` or `rss`. In an RSS feed a tag's text is read under the tag's name and its
  attributes under `tag@attribute`; a Torznab attribute (`<torznab:attr name="seeders" value="12"/>`)
  is `torznab:attr@seeders`, so any Torznab feed can be read. `rows` is a dotted path to the rows (`*` means every value of an
  object; a list found there is read element by element; empty means the body itself). `children`
  reads rows nested in each row, whose fields reach the outer row with `^.`.
- `fields` are `title` and `hash` (required), `magnet`, `size` (bytes, or text like `1.3 GB`)
  and `seeders`. Each is a dotted path, or one of: `{"template": "{^.title}[ ({^.year})]"}`
  (`{? ... ?}` vanishes when a hole inside is empty), `{"regex": {"from": path, "pattern": ...}}`
  (first capture group), `{"line": {"from": path, "skip": pattern, "index": n}}`,
  `{"coalesce": [...]}`, `{"join": [...]}`, `{"magnet_hash": path}`, `{"magnet_name": path}`,
  `{"magnet_size": path}`. Without a `magnet` field one is built from the hash and the trackers.
- `drop` removes rows (`{"path": ..., "equals": ...}` or `"starts_with"`); `must_contain_term`
  keeps only rows whose title holds the whole searched title; `dedupe` keeps one row per hash.

### Providers

```json
{ "id": "tvmaze", "kinds": ["series"], "base_url": "https://api.tvmaze.com",
  "types": {}, "sorts": {},
  "card":   { "id": "id", "title": "name", "year": { "year": "premiered" } },
  "list":   { "request": { "url": "{base}/schedule/full" }, "rows": "_embedded.show",
              "paging": { "style": "pool" } },
  "search": { "request": { "url": "{base}/search/shows?q={term}" }, "rows": "show" },
  "detail": { "request": { "url": "{base}/shows/{id}" }, "root": "" } }
```

- `card` says how a title is read, once, wherever the service gives one (a page, a search, a title
  in full, the titles related to it). Its `id` is the service's own id; the title's key is made from
  the kind and that.
- A request's `url` is the whole address. `{base}`, `{type}` (what `types` calls the kind), `{sort}`
  (what `sorts` calls the sort, `default` otherwise), `{id}`, `{term}`, `{page}`, `{per_page}`,
  `{skip}`, `{genre}`, `{genre_lower}` and `{year}` are filled in, each percent-encoded. A request's
  `url` starts with `{base}/`, so what is searched for can only ever go after the service's own
  address. A hole with
  nothing to fill it empties the address; `{? ... ?}` is left out when a hole inside is empty. A
  request can have `method`, `query` and a `json` body, where `{"$text": "..."}` is a text left as
  it is (a GraphQL document), `{"$number": "{page}"}` a number, `{"$sort": {"recent": ...}}` picks
  by sort, and a key ending in `?` is left out when its value is empty.
- `list.paging.style` is how the service pages: `page` (it takes `{page}` and `{per_page}`; `next`
  says whether more follows, by a `flag` or a `total`; `widen` asks for a wider page and filters here
  when the service cannot), `skip` (it takes `{skip}`; `window` reads a run of pages from the start
  and pages here when it cannot filter or order) or `pool` (it has no paging: one answer, paged
  here).
- What the service cannot do is done here: `dedupe`, `drop` (rows to leave out, with `after_today`
  for what is not out yet), `genre` and `year` (how a row names them, to filter on) and `order` (a
  list of keys, per sort, that a row is ordered by).
- `detail` says where the title is (`root`), which ids the service has (`id_pattern`), what a
  placeholder for a missing title looks like (`missing`), and where the rest is: `description`,
  `imdb_id`, `studios`, `trailer`, `episodes`, `cast`, `related` and `recommended` (the last two can
  have a `request` of their own). `check` says how an answer reports that it failed.
- Expressions are the same as for indexers, plus `const`, `by_kind`, `year`, `minutes`, `scale`,
  `add`, `positive`, `map` and `plain_text`. A template reads the row with `{path}`, the row around
  it with `{^.path}` and the kind with `{@kind}`.

A service that needs something these cannot say needs a change to the engine in
`core/src/providers/`, and so a new build.
