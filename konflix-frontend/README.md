# Konflix frontend

Next.js frontend for Konflix. It talks to the Konflix API (`../anime-server`) over
GraphQL and plays the video URL the API returns.

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
| `npm run dev` | Development server |
| `npm run build` | Production build |
| `npm start` | Production server on port 5000 |
| `npm run lint` | ESLint |

## How it fits together

```
pages/index.js            trending + recently aired episodes
pages/info/[id].jsx       anime details and trailer
pages/watch/[animeID]/[id].jsx
                          player, episode grid, prev/next
components/               presentational components
utils/graphql/client.js   the only place that talks to the API
utils/graphql/Queries.js  the GraphQL documents
utils/youtube.js          YouTube URL helpers
```

There is no API proxy and no API routes directory: video is served by the backend
directly, so the browser can play it without cross-origin workarounds.

## Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `NEXT_PUBLIC_GRAPHQL_URI` | `http://localhost:9000/graphql` | API address. Baked into the browser bundle at build time, so set it before `npm run build`. |
