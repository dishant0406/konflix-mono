# Attribution

## UI

The user interface in `konflix-frontend/` is derived from **Kitsune**
(https://github.com/Dovakiin0/Kitsune), which is licensed under the MIT License:

```
MIT License

Copyright (c) 2026 Dovakiin0

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

What was taken: the component structure and Tailwind styling of the card, row,
hero, episode-list and detail layouts, and the shadcn/ui primitives in
`src/components/ui/`.

What was **not** taken, and is deliberately absent:

- Kitsune's data layer (its API is a separate service that is not part of that
  repository). Every request in this app goes to the Konflix API.
- Its player (`kitsune-player.tsx`). Konflix uses its own HTML5 player with
  hls.js, because the release pipeline serves HLS for MKV sources.
- Its PocketBase-backed features: authentication, bookmarks, comments, profiles
  and continue-watching. Those need a fourth service and are not part of Konflix.
- Its `/api/proxy` route, which was an unauthenticated forward proxy (SSRF).
- Its Google Analytics tag.

## Data

Anime metadata comes from **AniList** (https://anilist.co) by default, and from
**Kitsu** (https://kitsu.io) when `METADATA_PROVIDER=kitsu` is set.
