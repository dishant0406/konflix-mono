# Konflix

Anime metadata and streaming, as two apps in one repository.

| Directory | What it is | Port |
| --- | --- | --- |
| `anime-server/` | GraphQL API. Metadata from AniList or Kitsu, releases from torrent indexers, video streamed from a torrent swarm or a debrid CDN. | 9000 |
| `konflix-frontend/` | Next.js app. Browse, search, and a player. | 3000 |

The previous version of this project scraped a private site and a public API, both
of which are gone. This version isolates every volatile upstream behind a
replaceable module, so a broken source degrades the app instead of taking it down.
Each app has its own README with the detail.

## Requirements

- Node 22+ (the torrent layer uses `webtorrent` 3)
- `ffmpeg` and `ffprobe` on `PATH` — used to inspect and remux video

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

The API must run on a host with a long-lived process, disk, and `ffmpeg` — a VPS or
container, not a serverless platform. It joins torrent swarms and streams video, and
neither works without a persistent process. Set `PUBLIC_URL` to the address clients
reach it on; stream URLs are built from it, and the server warns at boot if it is
still `localhost`.

The frontend is a standard Next.js app and can be deployed anywhere.

`anime-server/.env` holds a `TORBOX_API_KEY` if you enable the debrid source. It is
gitignored and must stay that way.
