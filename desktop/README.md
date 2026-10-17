<p align="center">
  <img src="../branding/akidev-icon.svg" alt="Akidev" width="128">
</p>

<h1 align="center">Akidev Desktop</h1>

A native macOS app for the same catalogue as the web sites: anime, series and films in one
window, with no hostnames, no modes and no server to run. The backend is a Rust library inside
the app. It looks titles up, finds a release on the torrent indexers, joins the swarm itself and
plays the video in the window.

The UI is [GPUI](https://www.gpui.rs/) through [gpui-kit](https://gpui-kit.com): a near-black
canvas, grey surfaces and one amber accent, so the artwork is the colour on screen. Video is
decoded by [libmpv](https://mpv.io).

The same library also runs the website's backend: `server/` is a REST server built on `core`, and
`../akidev-frontend` is the site that talks to it. See [server/API.md](server/API.md).

## What is in it

The window is one bar that is both the title bar and the navigation, with the screen below
it. The bar is transparent over artwork and turns solid once a page scrolls.

| Screen | What it does |
| --- | --- |
| Home | A rotating banner, a "Continue watching" row once you have started something, and rows from all three catalogues. A tile is clean at rest; hovering shows its facts and Play / Info |
| Anime, Series, Films | One catalogue with sort and genre filters |
| Search (⌘K) | Results as you type, across all three catalogues. Enter opens the best match, or the full results page when nothing is highlighted |
| Title | Banner, facts, Play and Trailer, episodes by season, cast, related and recommended titles |
| Watch | The picture fills the window. Controls hide after three seconds. Seek bar, ±10 seconds, volume, a source picker (every release found, play any), subtitles from the file and from online providers, audio tracks, an episode panel, next episode, fullscreen, and an offer of the next episode near the end |
| Watch party | The people icon in the bar opens a panel: start a party (you get a six-letter code and a link), or join one by its code. Everyone in a party watches or listens to the same thing and stays in step: what the host plays, guests play; a guest can pause, play and seek for everyone. Works between the app and the website, in any mix. See [Watch parties](#watch-parties) |
| Settings | Two sliders: how much disk the torrents may use, and how many are kept at once, a switch for update checks, and the version. Saved as soon as you let go |

Keys on the watch page: `space` pause, `←` `→` seek 10 seconds, `↑` `↓` volume, `m` mute,
`c` cycle subtitles, `a` audio, `e` episodes, `n` next episode, `f` fullscreen, `esc` leave
fullscreen or close the panel. Anywhere: `⌘K` search, `⌘Q` quit.

Progress is remembered per title, so opening an episode you left part way resumes it, and a
title you finish drops out of "Continue watching".

## Watch parties

The host is a device (this app, or a browser tab), not a server. Guests connect straight to it
with WebRTC, and only what to play and where travels: every device finds its own stream. The API
at `api.akidev.live` is just the meeting point where the first message each way is passed, and a
relay (Cloudflare's, see the root README) carries the connection for the networks that block direct
ones. The messages, and the rules that keep a guest in step, are in
[`party/PROTOCOL.md`](party/PROTOCOL.md); the meeting point's routes are in
[`server/API.md`](server/API.md).

`party/` is `akidev-party`: the messages, the sync rules and the WebRTC connection, for the app. The
website has the same in `akidev-frontend/src/lib/party`. `AKIDEV_API` points a development build at
another server (`AKIDEV_API=http://localhost:9000 cargo run -p akidev`).

## Layout

```
core/      akidev-core: providers, indexers, release ranking, torrent engine, stream resolver
sources/   the definitions of the indexers and providers, published separately from the app
party/  akidev-party: watch parties, the connection and the rules that keep guests in step
app/    akidev: the GPUI app and the libmpv player
```

In `app/src`, `updates.rs` is the pill and card and `updater.rs` the install step. `design/` holds the tokens (palette, type scale, spacing) and the few shared
components (buttons, chips, tabs, empty and error states); every screen is built from them.
`app.rs` is the window shell, `route.rs` is navigation, and each screen has its own file
(`home`, `browse`, `title`, `watch`, `settings`, plus `palette` for ⌘K). `player.rs` wraps
libmpv, `progress.rs` remembers where you are, and `images.rs` loads posters.

`core` has no GPU dependency, so everything in it is tested headless.

`core` is shared by two programs: this app, and `server/`, the website's backend. A fix to how a
release is found, ranked or opened lands in both. `core` began as a port of the Node API that
used to serve the site, and the server has since replaced that API.

| In `core` | What it does |
| --- | --- |
| `providers/`, one engine that follows the definitions in `sources/` (AniList, Cinemeta, TVMaze and YTS are definitions) | Lists, searches and details, cached in memory and on disk |
| `indexers/`, one engine that follows the definitions in `sources/` | Finds releases |
| `releases.rs` | Ranks them and picks a file out of a torrent |
| `torrent.rs`, `serve.rs` | Joins a swarm and reads a file from it, from any byte |
| `stream.rs` | Media id to a plan (the title and the ranked releases) and then to an open file |
| `search.rs` | Search across the three catalogues |
| `update.rs` | Finds a newer release on GitHub and downloads it, checked against its published hash |

Its tests are the Node tests carried over with the same inputs and expected outputs. What a browser
needs and mpv does not (HLS conversion, ffprobe, the debrid source, the Internet Archive) is in
`server/`, not here. mpv plays any container directly.

## The server

`server/` is `akidev-server`, a REST API over `core` with the video delivery a browser needs. It
is deployed with the site; see the root README. Run it with `cargo run -p akidev-server` from this
directory. [`server/API.md`](server/API.md) lists the routes and `server/src/config.rs` the settings,
and `cargo test -p akidev-server` runs its tests (they use `ffmpeg` and `ffprobe`, and say so and
pass when those are missing).

## Indexers and providers update without a build

Which indexers and providers exist, their addresses, how each is asked and how its reply is read,
and the public trackers are data in [`sources/sources.json`](sources/README.md),
not code. The app has one copy built in, uses the last good copy it saved, and fetches the
published one in the background (at start and every six hours). A downloaded copy is used only
if its ed25519 signature is valid, its version is higher than the one in use and it passes the
same validation as the built-in one; otherwise nothing changes, so a bad publish cannot break
an installed app. `./scripts/publish-sources.sh` signs and uploads a new copy to Cloudflare R2.
Viewers see none of this: there is no list of indexers, no switches and no addresses to change in the
app. Settings holds only the torrent limits and the update switch.

How a watch works: `stream.rs` turns a media id into search terms, asks every indexer that serves the kind,
ranks the releases and races the best few (the first starts at once, the next join every six
seconds if it is slow; a release nobody seeds is skipped). The winner's swarm is joined by
`torrent.rs`, which picks the file. `serve.rs` serves that file on `127.0.0.1`, behind a random
per-run token, with byte ranges, and the player reads from it.

**It streams, it does not download.** No file is ever selected, so the engine fetches only what
a reader asks for, plus a window ahead of it (about 32 MB, fixed inside librqbit), plus mpv's own
read-ahead (a minute, at most 64 MiB). Pieces behind the playhead stay on disk until the torrent
is evicted or you quit. When a file is chosen, its first and last few MB are fetched at once,
because players read both ends before showing a frame, and a stream is held open for as long as
the file is playable so librqbit keeps its peers connected. While buffering, the player shows the
transfer speed and peer count.

`vendor/librqbit` is librqbit 9.0.1 with one change (see `vendor/librqbit/PATCH.md`): starting or
seeking a stream wakes the peers that are idle. As published, an idle peer notices new work only
on a five second timer, so every seek and every new connection from the player waited up to five
seconds.

## Requirements

- macOS 12 or later on Apple silicon or Intel, with Xcode (GPUI renders with Metal), and `brew install mpv`; or
- Windows 10 or later (GPUI renders with DirectX), with Visual Studio's C++ tools and the Windows SDK
- Rust (stable)

## Windows

The same code builds for Windows. The window bar draws its own minimise, maximise and close
buttons, shortcuts use Ctrl where macOS uses Cmd, and the exe carries the icon. libmpv is the
`libmpv-2.dll` of the newest [mpv-winbuild](https://github.com/shinchiro/mpv-winbuild-cmake)
`mpv-dev` build, kept beside `Akidev.exe`.

```powershell
./scripts/bundle-windows.ps1    # target\release\bundle\Akidev-<version>-x86_64.zip
```

The script (run from a Visual Studio developer prompt) downloads libmpv, makes its import
library, builds and zips. The **Desktop** workflow runs it on every change, tests the code on
Windows, starts the app once and keeps a screenshot (artifact `akidev-windows`); the **Desktop
release (Windows)** workflow does the same for a `desktop-windows-v<version>` tag. The app is not signed,
so SmartScreen asks the first time ("More info", then "Run anyway").

From a Mac, `cargo xwin check -p akidev --target x86_64-pc-windows-msvc` (with `cargo install
cargo-xwin`, `brew install llvm lld`) type-checks the Windows build. It cannot link it: GPUI's
DirectX shaders are compiled by tools that only exist on Windows.

## Updates

The app looks for a newer version of itself about 20 seconds after it starts and then every six
hours (Settings, "Check for updates", on by default). It asks the GitHub releases of
`dishant0406/konflix-mono` (one constant, `REPO` in `core/src/update.rs`, so a rename is a one-line
change) for the newest non-draft, non-prerelease tag with this platform's prefix,
`desktop-mac-v<version>` or `desktop-windows-v<version>`, and compares it by number with the
running version (0.1.10 is newer than 0.1.9). When there is one, a small download icon with a
dot appears in the top bar, beside search (hover for "Update to 0.1.5"). Pressing it updates right
away: no card and no confirmation. A thin ring around the icon fills as the file downloads, turns
while it is checked and installed, and the tooltip names the stage. Nothing is downloaded until the icon is pressed, it is never drawn on the
watch page, and nothing is ever a modal. Settings has an About row with the version and the same
state in words ("Up to date", "0.1.5 available" with an **Update** button). Intel
Macs and other systems report nothing available.

Pressing the button downloads the installer from GitHub (only `github.com` and its asset hosts,
over https, redirects included) and keeps it only if its SHA-256 matches the `.sha256` published
beside it. The hash guards against a corrupt download; the signature is what is trusted:

- **macOS**: the disk image is mounted, and `Akidev.app` inside must pass `codesign --verify
  --deep --strict` and carry Team ID `689N5CNAYJ`. It is copied next to the running app, swapped
  in (the old bundle is put back if anything after the swap fails) and the app relaunches. If the
  app is not run from an `.app` bundle, or its folder is not writable (a translocated or
  read-only copy), nothing is installed and the download page is opened instead.
- **Windows**: the installer is run (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS
  /RESTARTAPPLICATIONS`) only when its Authenticode status is `Valid` and the signer is SignPath
  Foundation. While releases are unsigned, the release page is opened instead.

A failure turns the dot red, the tooltip says what happened in a sentence, and pressing the icon
again retries. When the update cannot be installed from here, pressing it opens the download page
and the tooltip says so. A Homebrew install updates itself the same way, since the cask installs the same notarised app into `/Applications`, which an
administrator can write to; `brew upgrade` keeps working too.

Two environment variables exist for testing the whole flow without a real install:
`AKIDEV_UPDATE_CURRENT=0.0.1` makes the app believe it is that version, and
`AKIDEV_UPDATE_BUNDLE=/path/to/Akidev.app` replaces that bundle instead of the running one.

## Installing with Homebrew

```bash
brew install --cask dishant0406/akidev/akidev
```

The cask lives in [dishant0406/homebrew-akidev](https://github.com/dishant0406/homebrew-akidev), and the
release workflow updates it. The app updates itself, so `brew upgrade` is not needed.

## Releasing

Run **Release desktop** from the repository's Actions tab: type a version such as `0.1.4` (empty
raises the patch number by one) and choose Windows, macOS or both. It writes the version into
`desktop/Cargo.toml`, commits it, tags `desktop-mac-v<version>` and `desktop-windows-v<version>`,
builds each platform (signing and notarising the Mac app, and signing the Windows installer once
SignPath is set up) and publishes a GitHub release for each, and points the Homebrew cask at the new Mac build. Pushing one of those tags by hand
does the same for that platform, if the version in `Cargo.toml` already matches.

## Running

```bash
cargo run -p akidev                          # the home screen
cargo run -p akidev -- search dangal         # open a screen directly
cargo run -p akidev -- series-tt0903747      # a title; add -s1e5 to play an episode
cargo run -p akidev -- settings
```

An id is `anime-<AniList id>`, `series-<IMDb id>` or `movie-<IMDb id>`, with `-ep3` or
`-s1e5` for an episode. A film's id plays it.

Set `RUST_LOG=info` to see which release each watch picked and how long it took, and
`RUST_LOG=akidev_core=debug` for the indexers and the torrent engine.

## Tests and checks

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The streaming test runs a seeder and a leecher on localhost, so it needs no internet. It fails if
the engine ever fetches the whole file again, or if a read waits on an idle peer.

## Packaging

```bash
./scripts/bundle-macos.sh   # target/release/bundle/Akidev.app
./scripts/dmg-macos.sh      # target/release/bundle/Akidev-<version>-<arch>.dmg
```

`bundle-macos.sh` builds the app with libmpv and every library it loads copied inside, so it runs
on a Mac without Homebrew, and with the Dock icon made from `assets/icon.png`. `dmg-macos.sh` packs
it into a disk image with an Applications shortcut and a short note.

The **Desktop release** workflow does this on GitHub: run it from the Actions tab, or push a tag
named `desktop-mac-v<version>` (the version in `app/Cargo.toml`), and the disk image is kept as the
`akidev-macos-dmg` artifact for 30 days. It is for Apple silicon.

The app is signed ad hoc, which is enough to run it locally. Without an Apple Developer ID
signature and notarisation, macOS asks the first time it is opened from a download: press **Open
Anyway** in System Settings > Privacy & Security, or run
`xattr -dr com.apple.quarantine /Applications/Akidev.app`. Signing and notarising need a
Developer ID certificate and an app-specific password as repository secrets, and a `codesign` and
`notarytool` step in the workflow.

libmpv is LGPL or GPL depending on how it was built. Homebrew's is built with GPL parts, so an
app bundling it is covered by the GPL. This project is AGPL-3.0.

## Settings and data

| What | Where |
| --- | --- |
| Settings | `~/Library/Application Support/live.akidev.Akidev/config.toml` (a file that cannot be read is kept as `config.toml.broken` and defaults are used) |
| Where you are in each title | `~/Library/Application Support/live.akidev.Akidev/progress.json` |
| Indexer and provider definitions | `~/Library/Caches/live.akidev.Akidev/sources/` (the last signed copy fetched; the built-in one is used when it is missing or older) |
| Posters | `~/Library/Caches/live.akidev.Akidev/images/` (files downloaded more than 30 days ago are removed at start; shrunk to the size they are shown at before they are kept) |
| Catalogue answers | `~/Library/Caches/live.akidev.Akidev/catalog/` (lists and title pages; shown at once for up to 24 hours while a newer copy is fetched, pruned after 2 days) |
| Torrent downloads | `<cache>/torrents/akidev-torrents/`, emptied at start and on quit and capped by the settings. It holds what was fetched while watching: the part played so far plus a little ahead, not the whole file. A `cache_dir` in `config.toml` moves the base directory; only the `akidev-torrents` folder inside it is ever deleted |

Every setting has a default, so no file is needed. The limits apply after a restart. A setting from
an older version that no longer exists, such as a list of turned-off indexers, is ignored.

## Known limits

- Playback is as good as the swarm. A 2160p release needs a couple of MB/s sustained, and the
  seeder counts indexers report are often far above the peers that actually connect, so such a
  release can take half a minute to show a frame and rebuffer. Nothing yet prefers a lighter
  release when the swarm is thin.
- A seek into a part that has not been fetched waits for the swarm to deliver it, usually a
  second or two.
- Frames are rendered in software at up to 1280 pixels wide and drawn by GPUI. That costs about
  a quarter of one core for 1080p, and a 4K picture is scaled down.
- Not every title has a torrent. The watch page then says so instead of showing a broken player.
- Tiles and controls are driven by the mouse; apart from the watch page keys there is no
  keyboard navigation of rows yet.
- The banner does not tint itself to the artwork's colours.
- macOS only so far. The code is not macOS specific, but Linux and Windows have not been built.

## Performance

- Home requests are prefetched at launch, and a title is prefetched when the pointer rests on its tile.
- Identical requests made together are sent once.
- Rows and tiles outside the screen are not built, so scrolling stays light and memory stays low.
- Posters are shrunk and re-encoded once, then served from disk.
