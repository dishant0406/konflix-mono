# Konflix API

GraphQL API for anime metadata plus an HTTP endpoint that streams video from
public torrent indexers.

The previous version scraped a private site and a public API, both of which are
gone. This version is built so that the volatile parts are isolated and
replaceable, and so that a broken upstream degrades instead of taking the app down.

## Requirements

- Node 20+ (Node 22+ to run the torrent layer, which is what `webtorrent` requires)
- `ffmpeg` and `ffprobe` on `PATH` — used to inspect video containers and convert them to HLS

## Setup

```bash
npm install
cp .env.example .env
npm start
```

Then open `http://localhost:9000/graphql`.

`npm start` and `npm run dev` load `.env` themselves (`--env-file-if-exists`), so
no extra tooling is needed and an absent `.env` is not an error. Running
`node index.js` directly bypasses that: use `node --env-file=.env index.js` or the
environment will not be read.

## Scripts

| Command | Purpose |
| --- | --- |
| `npm start` | Run the server |
| `npm run dev` | Run with automatic restart on file changes |
| `npm test` | Run the test suite |

## Endpoints

| Endpoint | Purpose |
| --- | --- |
| `POST /graphql` | Metadata and stream resolution |
| `GET /stream/:infoHash` | Video bytes for a resolved episode, relayed from the torrent swarm |
| `GET /dl/:token` | Serve a debrid file with range support. Used when `STREAM_SOURCES` includes `debrid` |
| `GET /health` | Provider/source/indexer/ffmpeg status. Add `?deep=1` to actually query the indexers. Reports each configured stream source, including a debrid token the API rejected. |

## Architecture

Four layers, each isolated so it can be replaced on its own.

```
graphql/providers/     metadata: what shows exist
  anilist.js             AniList GraphQL  (default)
  kitsu.js               Kitsu REST
  index.js               registry, selected by METADATA_PROVIDER

graphql/indexers/      torrent search: where releases are found
  subsplease.js          SubsPlease JSON API (every episode in one request)
  apibay.js              The Pirate Bay JSON API
  nyaa.js                Nyaa RSS (tries each mirror in NYAA_BASE_URLS)
  animetosho.js          AnimeTosho HTML search (not enabled by default)
  index.js               registry, selected by TORRENT_INDEXERS

graphql/stream/        video: turning a release into playable bytes
  index.js               episode id -> anime title -> search -> rank -> source
  releases.js            release name parsing and ranking (pure functions)
  torrent.js             webtorrent session and file selection
  media.js               ffprobe/ffmpeg wrappers
  hls.js                 HLS sessions: ffmpeg to segments, playlist and segment serving
  serve.js               routes: range serving for MP4, HLS for everything else
    sources/               where the bytes come from (see below)
      torrent.js             this server joins the swarm and relays them
      debrid.js              this server relays them from a debrid CDN
      index.js               registry, selected by STREAM_SOURCES
```

### Stream sources

Where the video bytes come from is a separate decision from which release is
chosen. Both sources relay through this server, for different reasons: the torrent
source has to, and the debrid source must because its CDN link carries the API key.

| Source | Path | Needs |
| --- | --- | --- |
| `torrent` | swarm → this server → browser | nothing |
| `debrid` | TorBox CDN → this server → browser | a TorBox API key |

`STREAM_SOURCES` lists them in preference order and the first that returns a
playable result wins, so `debrid,torrent` uses the cached copy when the episode is
already cached at the service and relays it from the swarm otherwise. A source that
is not usable — the debrid source without an API key — is skipped rather than
failing the request.

The debrid source only uses releases TorBox has **already cached**. An uncached
torrent would have to be fetched in full before anyone could watch it, and adding
one spends a separate 60-per-hour quota, so `createtorrent` is called with
`add_only_if_cached`, which refuses anything the service does not already hold.
When nothing is cached it reports that honestly and the next source takes over.

Measured against recent shows, 6–9 of the top 10 candidates are already cached, so
the debrid path usually resolves without touching the swarm at all.

### Adding a stream source

Create a file in `graphql/stream/sources/` that default-exports:

```js
{
  name: 'my-source',
  ready: () => true,                    // false when it cannot be used at all
  async resolve({ episodeNumber, ranked, deadlineMs }) {
    // ranked is the candidate release list, best first
    return { available: true, url, streamType: 'file', release }
    // or
    return { available: false, reason: 'why not' }
  },
}
```

Add it to the map in `sources/index.js` and list its key in `STREAM_SOURCES`.
`resolve()` must never throw: a failing source should reduce what is playable,
not fail the request. `streamType` is `file` for a source the browser plays
directly and `hls` for one this server converts into segments.

### Adding a metadata provider

Create a file in `graphql/providers/` that default-exports an object with these
five async functions, add it to the map in `providers/index.js`, and set
`METADATA_PROVIDER` to its key. Nothing else changes — the schema, the resolvers
and the frontend all stay as they are.

```
topAnime(page)             -> [{ id, title, trailer, imgUrl }]
animeSearch(term, page)    -> [{ id, title, trailer, imgUrl }]
animeDetails(id)           -> { id, name, type, status, totalEpisodes, released,
                                genres, synopsis, imageUrl, trailer,
                                episode_id: [{ episodeId, episodeNumber }] }
recentEpisodes(page)       -> [{ animeId, episodeId, name, episodeNum,
                                 subOrDub, imgUrl }]
recommandedAnime(genre)    -> [{ animeId, animeTitle, animeImg }]
```

`episodeId` is opaque to callers but must be parseable by `stream/index.js`. The
built-in providers use `<mediaId>-ep<number>`.

### Adding a torrent indexer

Create a file in `graphql/indexers/` that default-exports `{ name, async search(term) }`
returning `[{ title, infoHash, magnet, sizeBytes, seeders, source }]`, add it to
the map in `indexers/index.js`, and list its key in `TORRENT_INDEXERS`.

`search()` must never throw. A blocked or dead indexer should reduce the number of
results, not fail the request.

An indexer that answers with every episode of a show from one request sets
`titleOnly: true`. It is then called once with the show title instead of once per
episode-number form, which is what makes SubsPlease fast. `searchTerms()` in
`stream/releases.js` builds those forms.

## How streaming works

1. `streamLinkDetails(id)` resolves the episode to an anime title.
2. Every enabled indexer is searched for several forms of the episode number
   (`"<title> 1"`, `"<title> 01"`, `"<title> 001"`, `"<title> S01E01"`), because
   release groups write it differently and indexers match literally. Results are
   merged and de-duplicated by info hash. Searching only the first form was measured
   to be barely faster and to pick a worse release: for one show it ranked a 2-seeder
   release above a 56-seeder one, because the better release only appears under a
   different spelling of the episode number. An indexer that returns every episode at
   once (`titleOnly`) is asked once with the title alone.
3. Releases are filtered and ranked: the wrong show, HEVC/AV1, and episode
   mismatches are rejected. What survives is ordered by seeder health first (a
   release with no reachable peers cannot be played at all) and then by quality,
   where 1080p H.264 with a matching episode number wins.
4. The ranked candidates are handed to the configured stream source, which decides
   where the bytes come from:
   - **`torrent`** — the release is added to webtorrent, which fetches pieces on
     demand, so playback can start before the download finishes.
     `ffprobe` reads the container header through the download stream:
       - an H.264/AAC MP4 is served directly with HTTP range support (seeking works)
       - an MKV is re-packaged as HLS by `ffmpeg -c copy`, served as segments
       - anything else (HEVC, AV1) is rejected and the next release is tried
   - **`debrid`** — a release TorBox has already cached is turned into a direct CDN
     link, and the browser is given a short `/dl/:token` link. This server fetches
     that link and relays the bytes, because the CDN link carries the API key and
     must not be handed to the browser. It still avoids swarm membership and the
     torrent disk cache. An MKV from the CDN is re-packaged as HLS, which reads the
     CDN URL directly and so also keeps the key server-side.
5. If no source can serve the episode, the API returns `available: false` with a
   reason. That is a normal outcome, not an error, and the UI shows it honestly.
   `RESOLVE_DEADLINE_MS` bounds the *whole* resolve: it is turned into one deadline
   and each source is handed what is left of it, so adding a source cannot double the
   worst case. A source that would be asked after the deadline has passed is skipped.

Both `ffprobe` and `ffmpeg` read from stdin rather than a path on disk. A torrent's
file exists at full length as soon as it is added and is filled in as pieces arrive,
so a path is a file full of holes; reading through the download stream is what lets
these tools fetch only the pieces they actually need.

Torrents are evicted after `TORRENT_TTL_MINUTES` of inactivity, and at most
`MAX_TORRENTS` run at once or `MAX_CACHE_BYTES` of video is held, so disk and
sockets stay bounded. A torrent that is actively being streamed is never evicted.

## Configuration

See `.env.example` for the full list.

| Variable | Default | Meaning |
| --- | --- | --- |
| `PUBLIC_URL` | `http://localhost:9000` | Public address; stream URLs are built from it |
| `METADATA_PROVIDER` | `anilist` | `anilist` or `kitsu` |
| `STREAM_SOURCES` | `torrent` | Stream sources in preference order, e.g. `debrid,torrent` |
| `TORBOX_API_KEY` | _(unset)_ | Enables the `debrid` source. Needs a paid plan; a secret, server-side only |
| `DEBRID_CDN_HOSTS` | `tb-cdn.pw` | Hosts a debrid download link may be fetched from |
| `TORRENT_INDEXERS` | `subsplease,apibay,nyaa` | Enabled indexers |
| `INDEXER_TIMEOUT_MS` | `5000` | Per-indexer request bound |
| `NYAA_BASE_URLS` | `https://nyaa.si,https://nyaa.net` | Nyaa mirrors, tried in order |
| `CACHE_DIR` | `./.torrent-cache` | Download cache; cleared on startup |
| `MAX_TORRENTS` | `12` | Concurrent torrents |
| `MAX_CACHE_BYTES` | `8589934592` | Upper bound on bytes held on disk |
| `METADATA_TIMEOUT_MS` | `45000` | How long to wait for a magnet's file list |
| `DEBRID_TIMEOUT_MS` | `15000` | How long to wait for the debrid API |
| `RESOLVE_DEADLINE_MS` | `75000` | Upper bound on resolving one episode |

## Deployment

This must run on a host with a long-lived process and disk: a VPS or a container.
The `torrent` source cannot run on serverless platforms, because streaming needs a
persistent process, local disk for downloaded pieces, and `ffmpeg`.

Set `PUBLIC_URL` to the public address of this server, otherwise the stream URLs
handed to the browser will point at localhost.

### What debrid does and does not do

With `STREAM_SOURCES=torrent` every byte a viewer watches crosses this server. The
`debrid` source does **not** change that, and it is worth being precise about why,
because the obvious design looks like it should.

The TorBox CDN link contains the API key: the `token` query parameter **is** the key
(verified against the live API — the strings are identical, and the CDN rejects a
request without it). Redirecting the browser to that link would therefore publish the
account's key into the page, the browser history, and any screenshot, and anyone
holding it can read and modify the whole account. So this server fetches the CDN URL
itself, where the key stays, and relays the bytes to the viewer.

What debrid still buys, and it is real:

- **No swarm membership.** No DHT, no trackers, no dependence on peer connectivity —
  which is the thing that most often fails on a datacenter IP.
- **No torrent disk cache and no ffmpeg.** The bytes are a plain HTTP stream.
- **Faster to start.** A cached episode plays immediately instead of after a search
  for peers.

Two things to know before relying on it:

- The debrid account is metered, and **every byte a viewer fetches counts against
  it**. A single account is fine for you and a few friends; serving the public on
  one account is against the provider's terms, which explicitly name "mass account
  sharing" and "misguided attempts at public service". Abuse warnings rotate the API
  key, which breaks every link at once.
- The API key must stay server-side. The browser is given an opaque `/dl` token,
  never the CDN URL, and the token is generated from a CSPRNG.

#### Getting an account

The `debrid` source needs a paid TorBox plan. **The free plan cannot be used**:

- Free accounts have **no API key at all**. Since the v7.1 release, free users
  authenticate with a session token instead and "are no longer able to access the
  API", so the dashboard has no key to copy.
- Even if a token is supplied, free is limited to **1 download per 24 hours, 10 per
  month, and 1 concurrent slot** — not enough to stream from.

The cheapest way to test is the **$1 Pro trial: 24 hours of the full Pro plan**,
offered on the dashboard to free users who have never subscribed. It is a one-off
and does not auto-renew. Otherwise the cheapest plan that works is Essential at
$3/month.

1. Sign up at <https://torbox.app/signup> and confirm your email.
2. Open the dashboard at <https://torbox.app/dashboard> and start the **$1 Pro
   trial**. (The same button is not offered again once you have ever subscribed.)
3. Go to <https://torbox.app/settings> and scroll to the **API key** section. Copy
   the key — it is a long string, not your password. "Refresh" there rotates it and
   invalidates the old one.
4. Put it in `.env` as `TORBOX_API_KEY` and set `STREAM_SOURCES=debrid,torrent`.
5. Restart the server and confirm the token is accepted — `/health` reports the
   debrid source as `reachable: true`. A rejected token shows `reachable: false`
   with the reason, so a rotated or mistyped key is visible without trying to play
   something.

```bash
curl -s localhost:9000/health | node -e \
  "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>console.log(JSON.parse(d).sources))"
# good key: { debrid: { configured: true, reachable: true, torrents: 0 }, torrent: { configured: true } }
# bad key:  { debrid: { configured: true, reachable: false, error: 'Request failed with status code 403' }, ... }
```

Two API details matter if you test by hand:

- `requestdl` takes the token as a **query parameter** in addition to the
  `Authorization` header — the route declares it as required and does not inherit the
  bearer scheme. A header-only request is rejected with `422 Field required`. The
  source handles this.
- The CDN host is **not** `torbox.app`. Links are served from a `*.tb-cdn.pw` node,
  so `DEBRID_CDN_HOSTS` defaults to `tb-cdn.pw`. Allowlisting the API host instead
  rejects every real link with a 502. A link outside the list is refused and the
  reason is logged.

#### What is verified, and what is not

Verified against the live API with a real key:

- `checkcached`, `createtorrent` with `add_only_if_cached`, `mylist` and `requestdl`
  all return as documented; a cached release is added without waiting and an uncached
  one is refused.
- The CDN supports range requests: `Accept-Ranges: bytes`, `206 Partial Content`, and
  a correct `Content-Range` for both the first and a mid-file range. Seeking works
  through the relay.
- The CDN sends permissive CORS headers, reflecting any `Origin`.
- **Chrome plays the original file directly** — verified with an HEVC/FLAC `.mkv`,
  1920×1080, rendering frames. That is why the source accepts MKV rather than MP4
  only: anime releases are almost entirely MKV (measured: 12 of 12 cached files for
  one show, and 17/17, 45/47, 12/12 across three others), so an MP4-only rule would
  have made the source nearly useless.

Not verified:

- Whether a CDN link works when fetched from a **different IP** than the one that
  requested it. This no longer affects the design, because this server makes both the
  API call and the fetch, so the IPs always match.
- **TorBox's own transcode/HLS API is Pro-only.** `GET /v1/api/stream/createstream`
  answers Essential with `PLAN_RESTRICTED_FEATURE` ("User is not allowed to use web
  streaming"). It is not used here; direct playback of the original file covers the
  formats tested, including HEVC.

#### A note on the free plan and `debrid`

If the key is rejected, the debrid source reports itself unusable and the torrent
source takes over — the app keeps working. `/health` is the place to see which of
the two happened.

## Tests

```bash
npm test
```

The suite covers release parsing and ranking, indexer response parsing, magnet
construction, and the video pipeline (real `ffprobe`/`ffmpeg` runs against
generated files, plus HTTP range serving). Tests that would need live peers or
live indexers are excluded so the suite is deterministic.

## Legal

Metadata comes from public APIs (AniList, Kitsu). Video comes from public torrent
indexers and is not hosted by this project. Running this is your decision and your
responsibility; see the risks section in the project notes.
