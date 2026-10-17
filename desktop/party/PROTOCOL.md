# Watch party protocol

Devices that play the same thing together: films, series, anime and music. One device is the
**host**; the others are **guests**. The host can be the desktop app or a browser tab, and so can
a guest, in any mix.

## Who talks to whom

- A guest and the host connect with **WebRTC**, over one data channel labelled `party`. Every
  guest has its own connection to the host; guests do not talk to each other.
- `api.akidev.live` is only the **meeting point** (`desktop/server/API.md`, "Watch party"). The
  host opens a party and gets a six-letter code; a guest posts an offer for that code; the host
  takes it and posts an answer. After that the server is not involved, and it never sees what is
  played. Offers and answers are complete: the device waits until it has found its addresses (at
  most ten seconds) and sends them all at once.
- Nothing is streamed between devices. A message names what to play, and every device finds its
  own stream from that name.

## Messages

One JSON object per data-channel message, as text. `t` says which. The Rust list is
`party/src/protocol.rs`, the web one `akidev-frontend/src/lib/party/protocol.ts`.

| `t` | from | fields | meaning |
| --- | --- | --- | --- |
| `hello` | guest | `name` | First message: who is joining. |
| `state` | host | `item`, `playing`, `position`, `at` | What is playing. `position` is seconds, read at `at` (the host's clock, milliseconds since 1970). Sent when it changes, and every 2 seconds while playing. A guest who joins is sent the last one at once. |
| `members` | host | `members: [{ name, host, state }]` | Who is in the party, and how each guest is doing with what is playing: `state` is `ready`, `loading` or `trouble`. |
| `play`, `pause` | guest | | Asks the host to play or pause. |
| `seek` | guest | `position` | Asks the host to seek. |
| `status` | guest | `state`, `id` | How this device is doing with the item whose id is `id`: `ready` (can play it now), `loading` (still finding a source or buffering) or `trouble` (cannot find or play it). |
| `ping` | guest | `id`, `sent` | Measures the way and the clock difference. |
| `pong` | host | `id`, `sent`, `at` | The answer: `sent` as it came, `at` the host's clock. |
| `bye` | either | | Leaving. From the host it ends the party. |

`item` is `null` (nothing), `{ "kind": "video", "id": "series-tt0903747-s1e5" }` (a media id, as
in `desktop/server/API.md`), or `{ "kind": "music", "id": "<YouTube video id>", "title", "artist",
"cover" }` (the last three are only what a guest shows before its own copy has loaded).

Everyone can ask to play, pause or seek; only the host acts, and only the host chooses what plays. A guest that presses pause sends `pause`; the host pauses
and sends `state`, and every device, the asker included, follows it. That gives one order of
events, so two people pressing things at once cannot leave devices disagreeing.

## Keeping in step

A guest estimates the host's clock from `ping`/`pong`: the offset from the quickest round trip
seen, as `at + round_trip / 2 - now`. It acts on a `state` only once it has a reading, so a `state`
that comes first is kept until the first `pong`. From a `state` it works out where the host is now:
`position + (host_now - at)` when playing (moved on by at most 10 seconds, so a clock that is far
out cannot send a guest far away), `position` when paused. Then, against its own player:

| gap (own - host) | when playing | when paused |
| --- | --- | --- |
| under 0.15 s | nothing | nothing |
| 0.15 s to 0.8 s | run at 0.95 (ahead) or 1.05 (behind) | seek |
| over 0.8 s | seek | seek |

Both the Rust (`party/src/sync.rs`) and the web (`lib/party/sync.ts`) versions have tests for the
same cases.

A host that leaves what it was playing (closes the film, or the music stops being the party's)
publishes a `state` with `item: null` and `playing: false`; guests pause. A guest who joins late is
sent the last `state` moved on to now.

## Guests that are still loading

Finding a film can take a guest a while (a torrent has to be found and joined), and the host's
device may be quicker. A guest says how it is doing with an item in `status`, naming the item by
`id`. A guest that has said nothing about the item the host is playing now, or only about an
earlier one, is counted as loading it, so a new film shows every guest as loading until each
says `ready`. The host's list of members shows who is.

- **At the start of a film** a host with company waits: it stays paused until no guest is loading,
  at least one second (so the guests have time to say they are) and at most 45 seconds (so a guest
  that cannot load does not hold everyone). Then it plays, and the guests follow.
- **In the middle of a film** nothing waits. A guest that finishes loading later seeks to where
  the host is and carries on from there.
- **Music** does not wait: a track loads in seconds, and a guest that is late seeks to the host.

## When things go wrong

- A guest that cannot find a stream for the item sends `status` with `state: "trouble"`; the host's
  list of members marks it. Everyone else carries on, and the host does not wait for it.
- A connection that fails or closes removes the guest, and the host frees its place on the server.
- When the host leaves or its connection ends, the party ends: guests get `bye` or see the
  connection close. The host role is not handed on.
- Devices on networks that block direct connections need the TURN relay (Cloudflare's, set up
  with `CF_TURN_KEY_ID` and `CF_TURN_TOKEN` on the server). Without it they cannot join.
