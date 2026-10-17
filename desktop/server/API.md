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
  "active_torrents": 0, "hls_sessions": 0, "resolving": 0, "video_bytes_sent": 0, "cache_bytes": 0 }
```

`resolving` is how many source searches are running (out of 4), and `video_bytes_sent` is the video
this server has sent since it started, which is what to watch to know what the bandwidth costs.

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

Body `{ "id": "series-tt0903747-s1e1", "hevc": false, "start": 1200, "release": "<info hash>" }`. `hevc` says the browser
can play HEVC; the server then keeps HEVC video as it is, and converts it otherwise. `start` is
where the viewer means to begin, in seconds, and lets a conversion run that far ahead for a
resume. `release` is the `id` of a release the viewer picked from `GET /v1/releases`: that one is
played, and no other (`{ "available": false }` with a reason when it cannot be). All three are optional.

Finding a source searches indexers and can take up to a minute, so the request stays open.

```json
{ "available": true, "url": "https://api.akidev.live/s/<token>", "stream_type": "file",
  "release": "Breaking Bad S01E01 1080p", "source": "torrent", "duration": 3480.5,
  "subtitles": [{ "label": "English", "lang": "en", "url": "https://.../s/<token>/sub/0.vtt" }] }
```

or `{ "available": false, "reason": "No torrent found for \"...\"." }`. The reason is for the
viewer. `503` with `{ "error": "busy" }` when the server is at its limits; try again shortly. `429` when
the client has used up its allowance: the video of a day (`MAX_STREAM_BYTES_PER_DAY`, off unless
set) or the plays of ten minutes.

The origin of `url` is `STREAM_URL` when that is set (video can be served from a host of its own),
otherwise `PUBLIC_URL`.

`stream_type` is `file` (give `url` to a `<video>` element) or `hls` (attach hls.js to `url`;
Safari plays it natively). `duration` is seconds when known. `subtitles` is always empty now (kept so older clients parse the reply); use
`GET /v1/subtitles` instead. The URL is valid for a few hours; an expired one answers 404 and the player starts
over with a new `POST /v1/play`.

## `GET /v1/releases?id=<episode or film id>`

Every release found for an episode or a film, for a viewer to pick one: the best first, in the
order they would be tried, then the ones with a codec that may not decode where it is shown
(`uncommon`). The first call searches the indexers and can take ten seconds; later ones are kept.

```json
{ "releases": [{
    "id": "ab12...40 hex characters (the info hash; pass it to /v1/play as `release`)",
    "title": "Show S01E01 1080p WEB-DL x264 AAC",
    "size_bytes": 1400000000, "seeders": 66, "source": "knaben",
    "resolution": 1080, "codec": "h264", "audio": "compressed",
    "pack": false, "uncommon": false, "cached": true } ] }
```

`resolution` is 480, 720, 1080 or 2160; `codec` is `h264`, `hevc` or `av1`; `audio` is `lossless`
(FLAC, DTS, AC-3) or `compressed` (AAC, MP3, Opus); any of them may be `null` when the name does
not say. `size_bytes` may be `null`. `pack` is a whole season in one torrent. `cached` is whether the
debrid service already holds the release (it starts at once; the others wait for a swarm) and is
`null` when there is no such service or it could not be asked.

## `GET /v1/subtitles?id=<episode or film id>`

The subtitles there are for it, from the sources in the signed definitions (`subtitles` in
`sources.json`): a few per language, English first and then by language name.

```json
{ "subtitles": [{ "id": "opensubtitles:30949", "provider": "OpenSubtitles",
    "lang": "en", "language": "English", "label": "Show.S01E01.DVDRip.srt",
    "hearing_impaired": false,
    "url": "https://api.akidev.live/v1/subtitles/file/<token>.vtt" }] }
```

`lang` is a two-letter code (`pt-BR` and `zh-TW` where the region matters), `language` its name in
English. `url` is the file as WebVTT, on this server: give it to a `<track>` element. The list is
empty when no source has any, and an anime is looked up only when it is short and not a later season
(a source numbers the episodes of a long or later-season show differently).

## `GET /v1/subtitles/file/{token}.vtt`

What `url` above points at: `text/vtt`, kept for good. `404` when the token was not listed (ask for
the list again).

## `GET /s/{token}`, `/s/{token}/{file}`

What `url` points at: the video with `Range` support (`file`), or the playlist and its segments
(`hls`, `index.m3u8`, `init.mp4`, `seg00000.m4s`), and subtitle files (`sub/0.vtt`). A player
only needs the URL it was given.

## Music: `GET /v1/music/search?q=<term>`, `/v1/music/albums/{id}`, `/v1/music/artists/{id}`, `/v1/music/radio/{videoId}`, `/v1/music/stream/{videoId}`

Music comes from YouTube Music (the vendored `vendor/ytmapi-rs`, see its PATCH.md); the audio is found
with yt-dlp (`YT_DLP` names the binary; the image's copy updates itself daily).

- `search` answers `{ songs: Track[], albums: AlbumCard[], artists: ArtistCard[] }`. A track's `id` is
  the YouTube video id.
- `radio/{videoId}` answers `{ songs: Track[] }`: what YouTube Music plays after that track (without
  the track itself). A player that has run out of queue appends these, which is the autoplay.
- `albums/{id}` answers the album with its `tracks`. `artists/{id}` answers `songs`, `albums`,
  `singles` and `related`.
- `stream/{videoId}` is the audio, passed through with `Range` support (206), so the browser can seek.
  The address yt-dlp finds is cached for 30 minutes; the server fetches the bytes itself because the
  address only works from the machine that asked. Not compressed.
- Covers are YouTube hosts, which `IMAGE_HOSTS` allows, and go through `/v1/img` like every picture.

## Watch party: `/v1/party`

A party's host is a device, and guests connect to it directly with WebRTC (see
`party/PROTOCOL.md`). These routes are only the meeting point: they pass the first message each
way, an offer and an answer, and never see what is playing. A party is forgotten when its host
stops asking for guests for two minutes, or after twelve hours.

The host's secret is sent as `Authorization: Bearer <host_token>`.

| route | does |
| --- | --- |
| `POST /v1/party` | Opens a party: `{ "code": "K7QF92", "host_token": "..." }`. 20 an hour per client. `503` when the server holds too many. |
| `GET /v1/party/{code}/ice` | `{ "ice_servers": [...] }`, asked for by the host before each guest and by a guest before it makes its offer. `404` when the party is gone. |
| `POST /v1/party/{code}/join` | Body `{ "offer": "<sdp>" }`; answers `{ "guest": "<id>" }`. `404` gone, `409` full (20 guests). 60 per ten minutes per client. |
| `GET /v1/party/{code}/offers` | Host only. Waits up to 25 seconds, then answers `{ "offers": [{ "guest": "<id>", "offer": "<sdp>" }] }`, possibly empty. Each call counts as the host being there. |
| `POST /v1/party/{code}/answer/{guest}` | Host only. Body `{ "answer": "<sdp>" }`; `204`. `404` when no guest has that id. |
| `GET /v1/party/{code}/answer/{guest}` | The guest waits up to 25 seconds for `{ "answer": "<sdp>" }`; `204` when it has not come yet, ask again. `404` when the party is gone or no guest has that id. |
| `DELETE /v1/party/{code}/guests/{guest}` | Host only. A guest left: frees its place. `204`. A place the host has not been given the offer for in a minute is freed on its own. |
| `DELETE /v1/party/{code}` | Host only. Ends the party. `204`. |

Offers and answers are complete (the device waited, up to ten seconds, to have found its addresses),
at most 32 KB (`400` when empty or larger). `429` when a client asks about codes faster than a
person would (60 `ice` or answer requests in ten minutes, 60 joins, 20 parties an hour). `ice_servers` is the
WebRTC `iceServers` list. With `CF_TURN_KEY_ID` and `CF_TURN_TOKEN` set it is what Cloudflare's
Realtime TURN service gives for that key (its STUN servers and a TURN relay, with a login that
works for six hours, asked for at most every half hour). Without them, or when Cloudflare does
not answer, it is the STUN servers (`STUN_URLS`) alone, and devices on networks that block direct
connections cannot join; everything else works.

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
