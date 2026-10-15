# Akidev API

REST, JSON, UTF-8. The server is `akidev-server`, a Rust binary over `akidev-core`, the same
library the desktop app is built on. There is no GraphQL.

Base URL: `https://api.akidev.live` (`NEXT_PUBLIC_API_URL` in the frontend). CORS allows the
site's origin. Errors are `{ "error": "message" }` with a 4xx or 5xx status.

Ids are media ids, the same ones the app uses: `anime-16498`, `series-tt0903747`,
`movie-tt0111161` for a title, and `anime-16498-ep1`, `series-tt0903747-s1e5` for an episode.
A film is played by its own title id.

## `GET /health`

```json
{ "status": "ok", "version": "0.1.1", "ffmpeg": true, "sources": ["debrid", "torrent", "archive-org"],
  "debrid": { "configured": true, "reachable": true },
  "active_torrents": 0, "hls_sessions": 0, "cache_bytes": 0 }
```

## `GET /v1/home`

The whole home page in one request.

```json
{ "featured": [Media],
  "rows": [{ "title": "Trending Anime", "kind": "anime", "items": [Media] }] }
```

`featured` is the front of each catalogue interleaved, only titles with a `banner`, at most 9.
A row whose provider failed has `items: []`; the page leaves it out.

## `GET /v1/catalogue`

| query | values | default |
| --- | --- | --- |
| `kind` | `anime`, `series`, `movie`, `all` | `all` |
| `sort` | `popularity`, `trending`, `score`, `recent`, `title` | `popularity` |
| `genre` | a provider genre, e.g. `Action` | none |
| `page` | 1, 2, ... | 1 |

`kind=all` asks the three catalogues and interleaves them. Returns `{ "results": [Media],
"has_next": true }`. 24 titles a page.

## `GET /v1/search?q=<term>&limit=<n>`

Every catalogue is searched and the answers merged, best title matches first. `limit` defaults
to 60 and is capped at 60. Returns `{ "results": [Media] }`.

## `GET /v1/titles/{id}`

A title in full. 404 when the id is unknown.

```json
{ "media": Media, "studios": ["WIT STUDIO"],
  "trailer": { "site": "youtube", "id": "abc" },
  "episodes": [{ "id": "series-tt0903747-s1e1", "season": 1, "number": 1, "name": "Pilot", "airdate": "2008-01-20" }],
  "cast": [{ "name": "Bryan Cranston", "role": "Walter White", "image": "https://..." }],
  "related": [Media], "recommended": [Media] }
```

`episodes` is empty for a film and for an anime, whose episodes are `1..media.episodes`
(ids `anime-<id>-ep<n>`). `trailer` may be `null`.

## `POST /v1/play`

Body `{ "id": "series-tt0903747-s1e1", "hevc": false, "start": 1200 }`. `hevc` says the browser
can play HEVC; the server then keeps HEVC video as it is, and converts it otherwise. `start` is
where the viewer means to begin, in seconds, and lets a conversion run that far ahead for a
resume. Both optional.

Finding a source searches indexers and can take up to a minute, so the request stays open.

```json
{ "available": true, "url": "https://api.akidev.live/s/<token>", "stream_type": "file",
  "release": "Breaking Bad S01E01 1080p", "source": "torrent", "duration": 3480.5,
  "subtitles": [{ "label": "English", "lang": "en", "url": "https://.../s/<token>/sub/0.vtt" }] }
```

or `{ "available": false, "reason": "No torrent found for \"...\"." }`. The reason is for the
viewer. `503` with `{ "error": "busy" }` when the server is at its limits; try again shortly.

`stream_type` is `file` (give `url` to a `<video>` element) or `hls` (attach hls.js to `url`;
Safari plays it natively). `duration` is seconds when known. `subtitles` is empty when there
are none. The URL is valid for a few hours; an expired one answers 404 and the player starts
over with a new `POST /v1/play`.

## `GET /s/{token}`, `/s/{token}/{file}`

What `url` points at: the video with `Range` support (`file`), or the playlist and its segments
(`hls`, `index.m3u8`, `init.mp4`, `seg00000.m4s`), and subtitle files (`sub/0.vtt`). A player
only needs the URL it was given.

## Media

```json
{ "id": "anime-16498", "kind": "anime", "title": "Attack on Titan",
  "poster": "https://...", "banner": "https://...", "description": "...",
  "format": "TV", "year": 2013, "episodes": 25, "runtime_minutes": 24,
  "status": "FINISHED", "score": 85, "popularity": 123456,
  "genres": ["Action", "Drama"], "is_adult": false }
```

`kind` is `anime`, `series` or `movie`. `score` is 0 to 100. Every field except `id`, `kind`,
`title`, `genres`, `popularity` and `is_adult` may be `null`. `status` is a provider's own word
(`FINISHED`, `RELEASING`, `Ended`, ...), so treat it as display text.
