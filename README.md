# Konflix

Anime, series and films from one API, served as two sites.

| Site | Catalogue | Container |
| --- | --- | --- |
| `anime.konflix.live` | Anime (AniList) | `frontend` |
| `tv.konflix.live` | Films and series (Cinemeta, with TVMaze and YTS as fallbacks) | `frontend-tv` |
| `api.konflix.live` | GraphQL, shared by both | `api` |

| Directory | What it is | Port |
| --- | --- | --- |
| `anime-server/` | GraphQL API. Metadata from AniList/Kitsu/Cinemeta/TVMaze/YTS, releases from torrent indexers, video streamed from a torrent swarm or a debrid CDN. | 9000 |
| `konflix-frontend/` | Next.js app (App Router, TypeScript). Built twice, once per site. | 3000 / 3001 |

The previous version of this project scraped a private site and a public API, both
of which are gone. This version isolates every volatile upstream behind a
replaceable module, so a broken source degrades the app instead of taking it down.
Each app has its own README with the detail.

The frontend's UI is derived from [Kitsune](https://github.com/Dovakiin0/Kitsune)
(MIT); see [ATTRIBUTION.md](ATTRIBUTION.md) for exactly what was taken and what
was deliberately left behind.

## How one codebase serves two sites

Both sites are the **same image built twice**, with `NEXT_PUBLIC_SITE` (`anime` or
`tv`) selecting a config object in `konflix-frontend/src/constants/site.ts`. That
object holds everything that differs: the content type sent to the API, the row
titles, the genre list, the filter vocabulary, the attribution. Components read
`SITE` rather than branching on a site name.

This is a build argument rather than a runtime variable on purpose. Next.js inlines
`NEXT_PUBLIC_*` at build time, and the API address is identical for both sites, so
there is no host detection and no per-request branching.

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
agree on a form: AniList, TVMaze and YTS use numbers, Cinemeta uses IMDb ids
(`tt27548557`). A digits-only charset made every Cinemeta film unreachable from the
frontend, which parses the same ids.
`anime-server/lib/mediaId.js` and `konflix-frontend/src/lib/media-id.ts` must agree.

## Adding a provider or an indexer

Both are name-keyed maps, selected by environment variable, so adding one is a new
file and a key - no change to the schema, the resolvers, or the frontend.

| Map | Directory | Selected by |
| --- | --- | --- |
| Metadata providers | `anime-server/graphql/providers/` | `PROVIDERS` (per type, `+`-separated for a fallback chain), `METADATA_PROVIDER` |
| Torrent indexers | `anime-server/graphql/indexers/` | `TORRENT_INDEXERS` |
| Stream sources | `anime-server/graphql/stream/sources/` | `STREAM_SOURCES` |

Each entry declares the content types it serves (`types: ['anime']`, `['movie']`,
`['series']`, or several), so a film search never queries an anime feed.
`GET /health` reports every configured entry and which types it covers.

**A type may name several providers, most-preferred first.** They are separated by
`+` within one `PROVIDERS` entry, and the next answers when the first returns nothing
or throws - so an outage degrades a row instead of emptying it:

```
PROVIDERS=anime:anilist,series:cinemeta+tvmaze,movie:cinemeta+yts
```

This matters most for films. Cinemeta is a *catalogue* - every film that exists - while
YTS is a *torrent tracker*, whose catalogue is only what has been ripped. Searching YTS
for a film nobody had released returned zero results while the indexers held 277-seeded
releases for it, which is why a newly released film was unfindable. Cinemeta answers
first and YTS catches anything it is missing. TVMaze stays in the series chain because
its Indian television coverage is better than Cinemeta's.

## How a row and a search are ordered

Two rules worth knowing, because both were bugs and both are easy to reintroduce.

**A "newest" row means released, not announced.** AniList reports a null date for a
title that has not come out and sorts it first under its date sort, and Cinemeta's
date-ordered catalogue leads with announcements. Each provider therefore bounds its own
date sort: AniList excludes unreleased and undated titles, and Cinemeta drops a title
whose exact release date is in the future. A provider that cannot do this has no
`recent` sort offered for it, rather than a sort that quietly returns the wrong thing.

Cinemeta's date catalogue is also not contiguously ordered upstream - measured, its
first page mixes 2020, 2025 and 2026 and its second repeats rows from its first - so
that row is read as a window and sorted and paged here. Paging it upstream produced six
duplicate rows across two pages.

**A search spans every catalogue the site carries.** The API's search is scoped to one
content type, because a title is addressed by a provider-specific id, so a site declares
its catalogues in `searchContentTypes` (`konflix-frontend/src/constants/site.ts`) and the
search box and results page query each and interleave the answers. Searching one
catalogue would silently hide half the site: on the TV site "Breaking Bad" is a series
and "Dangal" is a film.

Results whose title starts with the term are moved ahead of fuzzy matches, because
TVMaze's own search returned "D.N.Angel" above "Dangal". `konflix-frontend/src/lib/merge-search.ts`
holds the merge and its tests.

## Known limitations

**A film is found by title, and a title is ambiguous.** Release names carry a bare
number for both the sequel ("Dhamaal 4", "Stree 2") and the year ("Singham Again 2024"),
so a film is identified by title *and* year, and the year decides which of several
same-titled releases is served. The film gate therefore keys on an explicit `SxxExx` or
a season token to tell a series episode from a film. Keying on a bare number instead
rejected seven of ten real films, which is why a film reported "no source" while the
indexers held 73-seeded releases for it.

**Bollywood coverage is thinner than Hollywood's, for a real reason.** Not every film
has a torrent. The catalogue can list a title the indexers have nothing for, and the
watch page then says so honestly rather than showing a broken player. That is the
ceiling of a torrent-based source layer, and no indexer change raises it.

**The Pirate Bay is reached through knaben, not directly.** knaben is a meta-search that
aggregates The Pirate Bay, YTS, 1337x and Nyaa, and its rows carry the tracker they came
from. Measured head-to-head on Indian films it returned a superset of a direct Pirate Bay
mirror with far higher seeder counts (Dhamaal 4: 18 hits at 216 seeders versus 3 at 22),
and the mirror's only unique rows had zero seeders. `apibay` - the Pirate Bay JSON API -
is registered but disabled: from a datacenter IP it answers 403 with a Cloudflare JS
challenge. A proxy does not fix that, because the challenge follows the IP rather than
the request, and neither does a headless browser, which costs a Chrome process per
request on a box that is also running `ffmpeg`. It works from a residential IP, so
re-enabling it is one env var.

**A converted stream starts after a short wait.** A release whose container a phone
cannot play (an MKV) is re-packaged as HLS: `ffmpeg` writes six-second segments plus a
playlist, and the player starts on the first one. That takes a few seconds, and the
watch page says "Preparing the stream for your browser" while it happens. Seeking works
normally once segments exist, including backwards. A directly playable MP4 is relayed
as-is with byte-range support and starts immediately.

This replaced a single live fragmented MP4, which could not be played on iOS at all: iOS
requires a media source to answer a byte-range request with a real `206` and a length,
and a stream still being produced has neither. That played on a desktop and showed a
play button that did nothing on a phone.

## Requirements

- Node 22+ (the torrent layer uses `webtorrent` 3)
- `ffmpeg` and `ffprobe` on `PATH` — used to inspect video and convert it to HLS

## Setup

```bash
npm install                                  # installs both workspaces
cp anime-server/.env.example anime-server/.env
cp konflix-frontend/.env.example konflix-frontend/.env
```

## Running

```bash
npm run dev      # both apps; API on :9000, frontend on :3000
```

The frontend needs the API running first. To run them separately:

```bash
npm run dev --workspace=konflix-api
npm run dev --workspace=konflix
```

## Scripts

| Command | Purpose |
| --- | --- |
| `npm run dev` | Both apps with automatic restart |
| `npm test` | Backend test suite |
| `npm run lint` | Frontend ESLint |
| `npm run build` | Frontend production build |

## Deployment

One server runs both apps. Four containers, defined in `docker-compose.yml`:

| Container | Role |
| --- | --- |
| `cloudflared` | The only thing reachable from the internet, over an outbound tunnel |
| `api` | GraphQL and the video relay (`:9000`, internal only) |
| `frontend` | Next.js, the anime site (`:3000`, internal only) |
| `frontend-tv` | Next.js, the films and series site (`:3001`, internal only) |

Three hostnames reach the same tunnel and are routed by hostname in its ingress
config: `anime.konflix.live` to `frontend`, `tv.konflix.live` to `frontend-tv`, and
`api.konflix.live` to the API. `konflix.live` still resolves to the anime site, so
existing links keep working. **No reverse proxy**, because the tunnel already routes
by hostname, and the browser loads the API directly so nothing sits in the video byte
path.

Nothing binds a public port. `cloudflared` dials out, so there is no open firewall
hole and no certificate to renew on the server.

### Why not Cloudflare Workers or Containers

The API needs raw TCP/UDP sockets for torrent peers, `ffmpeg` on `PATH`, a writable
disk for the torrent cache, and a process that lives for the length of an episode.
Workers and Pages provide none of those. Cloudflare Containers would technically
work but requires the Workers Paid plan, and its request-scoped model fits a
20-minute video stream poorly. Cloudflare's job here is DNS, TLS, and the tunnel.

### One-time setup

1. **Create the server.** Any 2 vCPU / 4GB box with an 80GB disk is enough. The
   reference deployment is an **AWS Lightsail `medium_3_0`** (4GB RAM / 2 vCPU /
   80GB SSD / 4TB transfer, ~$24/month) in `us-east-1`, created with:

   ```bash
   aws lightsail create-key-pair --key-pair-name konflix-lightsail --region us-east-1
   aws lightsail create-instances --region us-east-1 \
     --instance-names konflix --availability-zone us-east-1a \
     --blueprint-id ubuntu_24_04 --bundle-id medium_3_0 \
     --key-pair-name konflix-lightsail
   aws lightsail allocate-static-ip --static-ip-name konflix-ip --region us-east-1
   aws lightsail attach-static-ip --static-ip-name konflix-ip \
     --instance-name konflix --region us-east-1
   # Open SSH only. Nothing else is ever published.
   aws lightsail put-instance-public-ports --region us-east-1 --instance-name konflix \
     --port-infos '[{"fromPort":22,"toPort":22,"protocol":"tcp","cidrs":["0.0.0.0/0"]}]'
   ```

   Ubuntu 24.04 or Debian 12. **Attach a static IP** — without one, a stop/start
   changes the address and `DEPLOY_HOST` silently points at nothing.

2. **Provision it.** From this repository. Cloud images give you an unprivileged user
   with `sudo` rather than a root login, hence the `sudo -E`:

   ```bash
   ssh -i ~/.ssh/konflix_lightsail ubuntu@SERVER_IP 'sudo bash -s' < scripts/setup-server.sh
   ```

   This installs Docker, creates a `konflix` deploy user, restricts the firewall to
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
   ssh-keygen -t ed25519 -f ~/.ssh/konflix_deploy -C konflix-deploy
   ssh -i ~/.ssh/konflix_lightsail ubuntu@SERVER_IP \
     "DEPLOY_PUBLIC_KEY='$(cat ~/.ssh/konflix_deploy.pub)' sudo -E bash -s" \
     < scripts/setup-server.sh
   ```

5. **Add the repository secrets and variables** (Settings → Secrets and variables →
   Actions):

   | Kind | Name | Value |
   | --- | --- | --- |
   | Secret | `DEPLOY_HOST` | server IP |
   | Secret | `DEPLOY_USER` | `konflix` |
   | Secret | `DEPLOY_SSH_KEY` | contents of `~/.ssh/konflix_deploy` |
   | Secret | `DEPLOY_PORT` | `22` (optional) |
   | Secret | `CLOUDFLARE_TUNNEL_TOKEN` | printed by `scripts/cloudflare.sh` |
   | Secret | `TORBOX_API_KEY` | from torbox.app (optional) |
   | Variable | `PUBLIC_URL` | `https://api.konflix.live` |
   | Variable | `NEXT_PUBLIC_GRAPHQL_URI` | `https://api.konflix.live/graphql` |
   | Variable | `FRONTEND_URL` | `https://konflix.live` |

   Secrets are write-only; nobody can read them back out.

6. **Deploy.** Push to `main`, or run the **Deploy** workflow from the Actions tab.

### How a deploy works

`.github/workflows/deploy.yml` copies the commit to `/opt/konflix`, renders
`.env.deploy` from the repository secrets, and runs `scripts/deploy.sh` over SSH. The
images are built on the server rather than in CI, so there is no registry and no image
transfer. The script waits for the API and the frontend to report healthy, prints the
live configuration, and only then prunes the old images. A failed build leaves the
previous revision running.

The workflow then verifies through Cloudflare — the API health endpoint, the frontend,
and a real GraphQL query — so a broken tunnel or DNS fails the deploy rather than
passing silently.

### Operating it

```bash
cd /opt/konflix
docker compose ps                      # what is running
docker compose logs -f api             # follow the API
docker compose restart api             # restart one service

# Nothing is published to the host, so /health is read inside the container.
docker compose exec api node -e \
  "fetch('http://127.0.0.1:9000/health').then(r=>r.json()).then(h=>console.log(h))"
```

The torrent cache lives in the `torrent-cache` volume, capped by `MAX_CACHE_BYTES`.
It is scratch data: deleting it costs nothing but a re-download.

### When playback starts failing with a 502

The debrid provider rotates its CDN domain, and `DEBRID_CDN_HOSTS` allowlists it. A
rotation makes every playback fail with a 502 even though the API key is fine.

The API log names the rejected host:

```
[dl] refused a link outside DEBRID_CDN_HOSTS: host=nexus-144.enam.tb-cdn.io.
```

Copy that domain into `DEBRID_CDN_HOSTS` in `.env.deploy` and restart the API — no
rebuild needed. This is the only setting that has needed changing in production.

### Secrets

No secret is committed. `.env.deploy` is rendered on the server from GitHub secrets by
the workflow, is gitignored, and is written with mode `600`. `anime-server/.env` is
only for local development.
