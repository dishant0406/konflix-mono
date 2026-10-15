<p align="center">
  <img src="branding/akidev-icon.svg" alt="Akidev" width="128">
</p>

<h1 align="center">Akidev</h1>

Anime, series and films from one API, served as one site and one desktop app.

| Hostname | What it serves | Container |
| --- | --- | --- |
| `akidev.live` | The site: Home, Anime, Series and Films | `frontend` |
| `anime.akidev.live` | Retired: redirects to `akidev.live/anime` | `frontend` |
| `tv.akidev.live` | Retired: redirects to `akidev.live/films` | `frontend` |
| `api.akidev.live` | The REST API and the video | `api` |

| Directory | What it is | Port |
| --- | --- | --- |
| `desktop/server/` | The API: a Rust binary, `akidev-server`. Catalogue from AniList, Cinemeta and TVMaze, releases from torrent indexers, video from a debrid CDN, a torrent swarm or the Internet Archive, converted for the browser only as far as it has to be. See [desktop/server/API.md](desktop/server/API.md). | 9000 |
| `desktop/core/` | The library the server and the desktop app share: providers, indexers, ranking, the torrent engine. | |
| `akidev-frontend/` | Next.js app (App Router, TypeScript). One build, one container, every hostname. | 3000 |
| `desktop/app/` | The native macOS app, with the same backend built in. See [desktop/README.md](desktop/README.md). | |

The server and the desktop app are built on **one library**, so there is one copy of every indexer,
provider and ranking rule. A fix to how a release is found or ranked lands in both. This replaced
a Node API that duplicated all of it.

The frontend's UI began as a derivative of [Kitsune](https://github.com/Dovakiin0/Kitsune)
(MIT) and has since been redesigned to match the desktop app; see
[ATTRIBUTION.md](ATTRIBUTION.md).

## One site, and two retired hostnames

There is **one frontend image and one frontend container**, and one site: Home, Anime,
Series and Films tabs over the same catalogue the desktop app shows, with the same design
(`akidev-frontend/src/constants/site.ts` holds the catalogues, sorts and genres).

`anime.akidev.live` and `tv.akidev.live` are retired. The middleware
(`akidev-frontend/src/middleware.ts`) redirects them with a 308 to `/anime` and `/films` on
the bare zone, and keeps the path for anything deeper, so old title and watch links work.
They still have to reach the container, which is why the tunnel keeps their hostnames.

## Content types

Every media id is self-describing, which is what lets one API and one set of routes
serve three catalogues:

```
anime-16498-ep1     an anime, episode 1
series-169-s1e5     a series, season 1 episode 5
movie-tt27548557    a film, no episode
series-169          a title, for a detail page
```

The type is part of the id because the ids come from different providers and would
otherwise collide - TVMaze's show 1 and YTS's film 1 are different titles. It also
means the watch page knows what it is looking at from the URL alone.

The provider's own id is alphanumeric rather than digits-only because providers do not
agree on a form: AniList and TVMaze use numbers, Cinemeta uses IMDb ids
(`tt27548557`). `desktop/core/src/media_id.rs` and `akidev-frontend/src/lib/media-id.ts`
must agree.

## Adding a provider or an indexer

Both are *data*, not code: signed JSON definitions that the engine in `desktop/core` follows, so a
change to what is asked of an upstream does not need a release. They are in `desktop/sources/`;
see [desktop/sources/README.md](desktop/sources/README.md) for the format and for how they are
signed and published. The server and the app both load the signed copy from
`https://sources.akidev.live` and keep the last good one on disk.

A stream source is different: it is code, because where the bytes come from is a different
mechanism each time (`desktop/server/src/sources/` for debrid and the Internet Archive, and the
torrent engine in `desktop/core`). `STREAM_SOURCES` lists them in order of preference: `debrid`
serves from a CDN cache, `torrent` joins a swarm and relays, `archive-org` returns a direct HTTPS
URL. A source that is not usable - `debrid` without an API key - is skipped rather than failing
the request, and `GET /health` reports what is configured.

## How a row and a search are ordered

Two rules worth knowing, because both were bugs and both are easy to reintroduce.

**A "newest" row means released, not announced.** AniList reports a null date for a
title that has not come out and sorts it first under its date sort, and Cinemeta's
date-ordered catalogue leads with announcements. Each provider's definition therefore bounds its
own date sort: AniList excludes unreleased and undated titles, and Cinemeta drops a title
whose exact release date is in the future. A provider that cannot do this has no
`recent` sort offered for it, rather than a sort that quietly returns the wrong thing.

Cinemeta's date catalogue is also not contiguously ordered upstream - measured, its
first page mixes 2020, 2025 and 2026 and its second repeats rows from its first - so
that row is read as a window and sorted and paged here. Paging it upstream produced six
duplicate rows across two pages.

**A search spans every catalogue.** A title is addressed by a provider-specific id, so
`GET /v1/search` asks anime, series and films and interleaves the answers (`akidev_core::search`).
Searching one catalogue would silently hide a third of the site: "Breaking Bad" is a series and
"Dangal" is a film. Results whose title starts with the term are moved ahead of fuzzy matches,
because TVMaze's own search returned "D.N.Angel" above "Dangal".

## How a video reaches the browser

`POST /v1/play` finds a source and answers with a URL. Between the file and the URL, the server
reads the file's header (`ffprobe`, through the same reader the player will use, since a torrent's
file on disk is sparse until its pieces arrive) and takes the **lightest** delivery that works, in
this order (`desktop/server/src/media.rs`):

1. **Direct.** An MP4 with H.264 and AAC or MP3 is sent as it is, with range support. No work at
   all.
2. **Copy.** Anything else with video a browser can decode is repackaged as HLS segments with the
   video copied untouched, so the picture is bit for bit the source. The work is I/O.
3. **Audio only.** The audio is copied too when it is AAC, and becomes stereo AAC otherwise (FLAC,
   DTS and AC-3 do not play everywhere): a few percent of one core.
4. **Re-encode**, only for video a browser cannot decode: 10-bit H.264, HEVC on a browser that lacks
   it (the page says whether it has it), AV1, MPEG-4 part 2. CRF 22 `veryfast` at no more than 720p,
   at low priority, on two threads, and **one at a time**: a second is told the server is busy.

Releases that need a re-encode (10-bit, HEVC) or that are too big to download faster than they play
(a REMUX, or an episode over 3GB) are tried last, so most plays never reach step 4.

A conversion runs a bounded distance ahead of the viewer (a quarter of an hour for a copy, one
minute for an encode), so an episode that is abandoned halfway is not downloaded or written to the
end. The lead is counted in bytes against the viewer's position, which does not lag the way counting
finished segments does. Sessions are limited (four at once), shared between viewers of the same file,
destroyed after half an hour idle and counted against the disk budget, oldest first.

This replaced a single live fragmented MP4, which could not be played on iOS at all: iOS requires a
media source to answer a byte-range request with a real `206` and a length, and a stream still
being produced has neither.

## Known limitations

**A film is found by title, and a title is ambiguous.** Release names carry a bare
number for both the sequel ("Dhamaal 4", "Stree 2") and the year ("Singham Again 2024"),
so a film is identified by title *and* year, and the year decides which of several
same-titled releases is served. The film gate therefore keys on an explicit `SxxExx` or
a season token to tell a series episode from a film.

**Bollywood coverage is thinner than Hollywood's, for a real reason.** Not every film
has a torrent. The catalogue can list a title the indexers have nothing for, and the
watch page then says so honestly rather than showing a broken player. That is the
ceiling of a torrent-based source layer, and no indexer change raises it.

**Seeking in a converted episode is limited to what has been converted.** HLS can only seek within
the segments that exist, so the seek bar grows as the episode is converted, and a long jump ahead
waits for the conversion to get there. A resume deep into an episode tells the server where to
start, so it runs that far at once. A direct MP4 seeks anywhere.

**Regional and Indian series are released as season packs, not episodes.** A pack names
the season instead of the episode ("Gyaarah Gyaarah S01 [1080p ...]", "Panchayat S02
Complete ..."), and three separate things had to be true before one could be served:

- The search has to ask for it. knaben requires every word of a term to match, so
  "Mirzapur S01E01" returns 0 hits where "Mirzapur S01" returns 15 and "Mirzapur"
  returns 100. `series_search_terms` therefore emits the season form and the bare title
  after the episode forms, so the episode-specific terms still decide the ordering
  whenever they return anything.
- The ranking has to admit it. A pack is a *collection*, and `is_batch` correctly says
  so, so packs are offered as a separate last-resort group behind both a release that
  names the episode and a single-file fallback.
- Its season has to be checked. `parse_season` reads `SxxExx`, a bare `Sxx` and `Season N`, and a
  file naming a different season is refused outright. For anime, whose episode ids are a flat count
  with no season, only a first-season pack is admitted: "Shippuden S02" holds episodes
  33-53 but names its files 01-21, so reading file 01 out of it would serve episode 33
  as episode 1.

`torrents-csv` is the indexer that makes these findable at all - it returned 4 rows
(best 71 seeders) for "Gyaarah Gyaarah" where knaben, eztv, nyaa and subsplease each
returned nothing. It is searched by title alone, because the endpoint matches a query as
a whole phrase and has no episode syntax. It also has **no category field and no
safe-search** - a query for "porn" returns adult rows - so its own term-relevance gate
is the only filter, and it is deliberately stricter than the downstream one.

**The Pirate Bay is reached through knaben, not directly.** knaben is a meta-search that
aggregates The Pirate Bay, YTS, 1337x and Nyaa, and its rows carry the tracker they came
from. Measured head-to-head on Indian films it returned a superset of a direct Pirate Bay
mirror with far higher seeder counts (Dhamaal 4: 18 hits at 216 seeders versus 3 at 22),
and the mirror's only unique rows had zero seeders. `apibay` - the Pirate Bay JSON API -
is defined but disabled: from a datacenter IP it answers 403 with a Cloudflare JS
challenge. A proxy does not fix that, because the challenge follows the IP rather than
the request. It works from a residential IP.

## A title with an apostrophe, and the Internet Archive fallback

Two independent problems made a title like "India's Got Latent" unfindable, and either
one alone was enough to produce "no source found".

**The apostrophe was destroyed before the search.** Title normalisation rewrites every
non-alphanumeric character to a space, so the title became "india s got latent" - a
string that matches nothing, because real releases are named "Indias Got Latent". The
damage was two-sided: the same normalisation rejected correct releases at the relevance
gate, so even a release that *was* found was thrown away. Titles are now matched in all
three spellings (`title_variants`), and all three are searched. Measured against archive.org, each
spelling returns a *different* set of items, so the union is what finds the episodes.

**No torrent indexer held the title at all.** knaben, torrents-csv, eztv, nyaa and
subsplease each returned nothing. `archive-org` is a last-resort stream source for exactly this
case. It is not a torrent index and not a debrid service: the Internet Archive stores the file and
serves its own browser-playable h264 MP4 derivative over plain HTTPS with real byte-range support,
so the bytes go straight from archive.org to the viewer. It is listed last in `STREAM_SOURCES`.

Its gate refuses to guess. A search for a show also returns news clips and reviews about
it, and a clip holds exactly one video file naming no episode - so "the item has one
video, so it must be the episode" served a 650MB news package as S2E5. An item is now
served only when its title or its file name actually names the requested episode.

## Requirements

- Rust (stable) for the server, and `ffmpeg` and `ffprobe` on `PATH`: they inspect video and convert it
- Node 22+ for the site

## Setup

```bash
npm install                                    # the site
cp akidev-frontend/.env.example akidev-frontend/.env
```

The server reads its settings from the environment and every one has a default that suits a small
server: see `desktop/server/src/config.rs` for the list (`PORT`, `PUBLIC_URL`, `CORS_ORIGINS`,
`CACHE_DIR`, `STREAM_SOURCES`, `TORBOX_API_KEY`, `MAX_TORRENTS`, `MAX_CACHE_BYTES`,
`MAX_HLS_SESSIONS`, `MAX_VIDEO_TRANSCODES`, ...).

## Running

```bash
npm run dev:api  # the server on :9000
npm run dev      # the site on :3000 (start the server first)
```

## Scripts

| Command | Purpose |
| --- | --- |
| `npm run dev:api` | The server |
| `npm run dev` | The site, with automatic restart |
| `npm test` | The site's tests, then the server's and the library's |
| `npm run lint` | Frontend ESLint |
| `npm run build` | Frontend production build |

## Deployment

One server runs the site and the API. Three containers, defined in `docker-compose.yml`:

| Container | Role |
| --- | --- |
| `cloudflared` | The only thing reachable from the internet, over an outbound tunnel |
| `api` | The REST API and the video (`:9000`, internal only) |
| `frontend` | Next.js, the site (`:3000`, internal only) |

Four hostnames reach the same tunnel and are routed by hostname in its ingress
config: `anime.akidev.live`, `tv.akidev.live` and `akidev.live` all go to
`frontend`, and `api.akidev.live` to the API. The two retired hosts have to reach the
container so it can redirect them. **No reverse proxy**, because the tunnel already routes by
hostname, and the browser loads the API directly.

Nothing binds a public port. `cloudflared` dials out, so there is no open firewall
hole and no certificate to renew on the server. The torrent engine only dials out as well.

### Why not Cloudflare Workers or Containers

The server needs raw TCP/UDP sockets for torrent peers, `ffmpeg` on `PATH`, a writable
disk for the torrent cache, and a process that lives for the length of an episode.
Workers and Pages provide none of those. Cloudflare Containers would technically
work but requires the Workers Paid plan, and its request-scoped model fits a
20-minute video stream poorly. Cloudflare's job here is DNS, TLS, and the tunnel.

### One-time setup

1. **Create the server.** Any 2 vCPU / 4GB box with an 80GB disk is enough. The
   reference deployment is an **AWS Lightsail `medium_3_0`** (4GB RAM / 2 vCPU /
   80GB SSD / 4TB transfer, ~$24/month) in `us-east-1`, created with:

   ```bash
   aws lightsail create-key-pair --key-pair-name akidev-lightsail --region us-east-1
   aws lightsail create-instances --region us-east-1 \
     --instance-names akidev --availability-zone us-east-1a \
     --blueprint-id ubuntu_24_04 --bundle-id medium_3_0 \
     --key-pair-name akidev-lightsail
   aws lightsail allocate-static-ip --static-ip-name akidev-ip --region us-east-1
   aws lightsail attach-static-ip --static-ip-name akidev-ip \
     --instance-name akidev --region us-east-1
   # Open SSH only. Nothing else is ever published.
   aws lightsail put-instance-public-ports --region us-east-1 --instance-name akidev \
     --port-infos '[{"fromPort":22,"toPort":22,"protocol":"tcp","cidrs":["0.0.0.0/0"]}]'
   ```

   Ubuntu 24.04 or Debian 12. **Attach a static IP** — without one, a stop/start
   changes the address and `DEPLOY_HOST` silently points at nothing.

2. **Provision it.** From this repository. Cloud images give you an unprivileged user
   with `sudo` rather than a root login, hence the `sudo -E`:

   ```bash
   ssh -i ~/.ssh/akidev_lightsail ubuntu@SERVER_IP 'sudo bash -s' < scripts/setup-server.sh
   ```

   This installs Docker, creates a `akidev` deploy user, restricts the firewall to
   SSH, and raises the open-file limit for concurrent streams. Re-running is safe.

3. **Create the tunnel and DNS.**

   ```bash
   cf auth login                 # if not already authenticated
   ./scripts/cloudflare.sh       # prints the tunnel token
   ```

   This creates the tunnel, sets its ingress rules, points every hostname at it with
   proxied CNAMEs, and sets TLS mode to `full`.

4. **Create an SSH key for deploys** and authorise it on the server:

   ```bash
   ssh-keygen -t ed25519 -f ~/.ssh/konflix_deploy -C akidev-deploy
   ssh -i ~/.ssh/akidev_lightsail ubuntu@SERVER_IP \
     "DEPLOY_PUBLIC_KEY='$(cat ~/.ssh/konflix_deploy.pub)' sudo -E bash -s" \
     < scripts/setup-server.sh
   ```

5. **Add the repository secrets and variables** (Settings → Secrets and variables →
   Actions):

   | Kind | Name | Value |
   | --- | --- | --- |
   | Secret | `DEPLOY_HOST` | server IP |
   | Secret | `DEPLOY_USER` | `akidev` |
   | Secret | `DEPLOY_SSH_KEY` | contents of `~/.ssh/konflix_deploy` |
   | Secret | `DEPLOY_PORT` | `22` (optional) |
   | Secret | `CLOUDFLARE_TUNNEL_TOKEN` | printed by `scripts/cloudflare.sh` |
   | Secret | `TORBOX_API_KEY` | from torbox.app (optional) |
   | Variable | `PUBLIC_URL` | `https://api.akidev.live` |
   | Variable | `NEXT_PUBLIC_API_URL` | `https://api.akidev.live` |
   | Variable | `FRONTEND_URL` | `https://akidev.live` |

   Secrets are write-only; nobody can read them back out.

6. **Deploy.** Push to `main`, or run the **Deploy** workflow from the Actions tab.

### How a deploy works

> **Names on the server.** The compose project (`konflix`), the image names, the checkout at
> `/opt/konflix` and the `konflix` deploy user were created before the project became Akidev, and
> they still are what the running server uses. Renaming them is a migration of the host, not a
> change to this repository, so they are left alone.

`.github/workflows/deploy.yml` runs in four jobs:

1. **verify** runs the frontend's tests, lint, type check and build, then the server's `cargo fmt`,
   `clippy` and tests (with ffmpeg installed, so the conversion path is really exercised).
2. **build-server** compiles `akidev-server` inside `rust:1-bookworm`, the Debian release the
   runtime image is based on, so it links the libc and OpenSSL it will run against. It then starts
   the binary once, so one that cannot run is found there.
3. **deploy** copies the commit to `/opt/konflix`, puts the binary in `desktop/server/dist/`,
   renders `.env.deploy` from the repository secrets and runs `scripts/deploy.sh` over SSH.
4. The same job then verifies through Cloudflare.

The server is built in CI and not on the host because a release build needs gigabytes of memory
for minutes, and the host is a 4GB box that is also serving. The api image only unpacks a 20MB
binary and installs ffmpeg. The frontend image is still built on the host. The script waits for
both to report healthy, prints the live configuration, and only then prunes the old images. A
failed build leaves the previous revision running.

The binary is built for x86-64, which is what the reference server is; `scripts/deploy.sh` refuses
to run on any other architecture with a message saying so.

The verification goes through Cloudflare, so a broken tunnel or DNS fails the deploy rather than
passing silently. It checks the health endpoint, the site and the two redirects, the catalogue and
title routes, and then finds real sources: an anime episode (and fetches its first bytes), a series
episode out of a season pack, a title only the id-searched indexer can find, and a later season
that must *not* resolve to the wrong one.

### Operating it

```bash
cd /opt/konflix
docker compose ps                      # what is running
docker compose logs -f api             # follow the API
docker compose restart api             # restart one service

# Nothing is published to the host, so /health is read from the frontend container, which
# sits on the same network (the api image has no shell tools beyond its own binary).
docker compose exec frontend node -e \
  "fetch('http://api:9000/health').then(r=>r.json()).then(h=>console.log(h))"
```

The torrent downloads and the converted video live in the `torrent-cache` volume, capped by
`MAX_CACHE_BYTES` together. It is scratch data, emptied at every start: deleting it costs nothing
but a re-download.

### When debrid playback stops working

The debrid provider rotates its CDN domain, and `DEBRID_CDN_HOSTS` allowlists it. A
rotation makes every debrid playback fail even though the API key is fine, and the server falls
back to the torrent relay.

The API log names the rejected host:

```
[debrid] refused a link outside DEBRID_CDN_HOSTS: host=nexus-144.enam.tb-cdn.io.
```

Copy that domain into `DEBRID_CDN_HOSTS` in `.env.deploy` and restart the API — no
rebuild needed. This is the only setting that has needed changing in production.

### Secrets

No secret is committed. `.env.deploy` is rendered on the server from GitHub secrets by
the workflow, is gitignored, and is written with mode `600`. Locally, the server reads its
settings from the environment (see `desktop/server/src/config.rs`).
