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

One server runs both apps. Three containers, defined in `docker-compose.yml`:

| Container | Role |
| --- | --- |
| `cloudflared` | The only thing reachable from the internet, over an outbound tunnel |
| `api` | GraphQL and the video relay (`:9000`, internal only) |
| `frontend` | Next.js (`:3000`, internal only) |

Two hostnames reach the same tunnel and are routed by hostname in its ingress config:
`konflix.live` to the frontend, `api.konflix.live` to the API. **No reverse proxy**,
because the tunnel already routes by hostname, and the browser loads the API directly
so nothing sits in the video byte path.

Nothing binds a public port. `cloudflared` dials out, so there is no open firewall
hole and no certificate to renew on the server.

### Why not Cloudflare Workers or Containers

The API needs raw TCP/UDP sockets for torrent peers, `ffmpeg` on `PATH`, a writable
disk for the torrent cache, and a process that lives for the length of an episode.
Workers and Pages provide none of those. Cloudflare Containers would technically
work but requires the Workers Paid plan, and its request-scoped model fits a
20-minute video stream poorly. Cloudflare's job here is DNS, TLS, and the tunnel.

### One-time setup

1. **Create the server.** Any 2 vCPU / 4GB box with a 40GB disk is enough (a Hetzner
   CX22 is about €4/month). Ubuntu 24.04 or Debian 12.

2. **Provision it.** From this repository:

   ```bash
   ssh root@SERVER_IP 'bash -s' < scripts/setup-server.sh
   ```

   This installs Docker, creates a `konflix` deploy user, restricts the firewall to
   SSH, and raises the open-file limit for concurrent streams. Re-running is safe.

3. **Create the tunnel and DNS.**

   ```bash
   cf auth login                 # if not already authenticated
   ./scripts/cloudflare.sh       # prints the tunnel token
   ```

   This creates the tunnel, sets its ingress rules, points both hostnames at it with
   proxied CNAMEs, and sets TLS mode to `full`.

4. **Create an SSH key for deploys** and authorise it on the server:

   ```bash
   ssh-keygen -t ed25519 -f ~/.ssh/konflix_deploy -C konflix-deploy
   ssh root@SERVER_IP "DEPLOY_PUBLIC_KEY='$(cat ~/.ssh/konflix_deploy.pub)' bash -s" \
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
curl -s localhost:9000/health          # configuration and upstream status
docker compose restart api             # restart one service
```

The torrent cache lives in the `torrent-cache` volume, capped by `MAX_CACHE_BYTES`.
It is scratch data: deleting it costs nothing but a re-download.

### Secrets

No secret is committed. `.env.deploy` is rendered on the server from GitHub secrets by
the workflow, is gitignored, and is written with mode `600`. `anime-server/.env` is
only for local development.
