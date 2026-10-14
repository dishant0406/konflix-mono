# Konflix frontend

Next.js app (App Router, TypeScript) for Konflix. It talks to the Konflix API
(`../konflix-api`) over GraphQL and plays the video URL the API returns.

The UI is derived from [Kitsune](https://github.com/Dovakiin0/Kitsune) (MIT). See
[../ATTRIBUTION.md](../ATTRIBUTION.md) for what was taken and what was left behind.

## Setup

```bash
npm install
cp .env.example .env      # point NEXT_PUBLIC_GRAPHQL_URI at the API
npm run dev               # http://localhost:3000
```

The API must be running first. By default the frontend expects it at
`http://localhost:9000/graphql`.

## Scripts

| Command | Purpose |
| --- | --- |
| `npm run dev` | Development server on :3000 |
| `npm run build` | Production build (standalone output) |
| `npm start` | Serve the production build |
| `npm run lint` | ESLint |

## Routes

| Route | What it is |
| --- | --- |
| `/` | Hero, trending, new releases, featured collections, genre rows |
| `/search` | Search with filters, and browsing when there is no term |
| `/anime/[id]` | Details, episode list, characters, related, recommendations |
| `/anime/watch/[animeID]/[id]` | The player, episode list and details |

`/info/[id]` and `/watch/[animeID]/[id]` were the previous paths. They still exist
and forward to the routes above, so old links and browser histories keep working.

## How data is fetched

`src/lib/queries.ts` holds every GraphQL document, one per shape, and
`src/query/*.ts` wraps them in react-query hooks. The API is the contract;
`src/types/media.ts` mirrors its schema.

Two deliberate choices:

- **The watch page fetches metadata and the stream separately.** Metadata takes a few
  hundred milliseconds; resolving a stream searches torrent indexers and can take much
  longer. Awaiting both together would leave the page blank the whole time, so the title
  and episode list render as soon as metadata arrives.
- **`available: false` is data, not an error.** The API returns a reason when it finds no
  playable release, and the page shows that instead of a broken player.

## The player

`src/components/anime/video-player.tsx` renders one `<video>` element and its controls.
It branches on the `streamType` the API reports:

- **`hls`** — the release's container is not one a browser can play (an MKV), so the API
  serves an HLS playlist. Chrome and Firefox need hls.js. Safari plays HLS natively, and
  on iOS hls.js cannot use MediaSource at all, so there it is left to Safari — that is
  what keeps inline playback and fullscreen working on a phone.
- **`file`** — bytes the browser plays directly, set as the element's `src`.

## Environment

| Variable | Default | Notes |
| --- | --- | --- |
| `NEXT_PUBLIC_GRAPHQL_URI` | `http://localhost:9000/graphql` | Baked into the browser bundle at build time, so changing it needs a rebuild |
| `NEXT_PUBLIC_GRAPHQL_TIMEOUT_MS` | `90000` | Sits just above the API's own resolve deadline |
