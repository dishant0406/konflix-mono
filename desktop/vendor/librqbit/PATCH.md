# librqbit 9.0.1, patched

This is librqbit 9.0.1 from crates.io (Apache-2.0, <https://github.com/ikatson/rqbit>) with one
change, wired in through `[patch.crates-io]` in the workspace `Cargo.toml`. The examples and the
test resources were removed to keep the copy small.

**The change.** A peer that has nothing to ask for waits on `new_pieces_notify`, with a five second
timeout. Starting a stream or seeking one makes pieces newly urgent but did not notify, so an idle
peer only noticed when its timer came round: every seek in a video waited up to five seconds for
its first bytes. `TorrentStateLive::notify_new_pieces` (`src/torrent_state/live/mod.rs`) now fires
when a stream is created and when one seeks (`src/torrent_state/streaming.rs`).

The upstream code has a TODO for exactly this ("wait for a notification of interest, e.g. update of
selected files or new streams"). Drop this copy, and the `[patch]` entry, once upstream does it.
