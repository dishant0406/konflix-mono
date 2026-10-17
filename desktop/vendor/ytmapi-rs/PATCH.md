# ytmapi-rs 0.3.3, vendored

This is `ytmapi-rs` 0.3.3 from crates.io (MIT, <https://github.com/nick42d/youtui>), with its helper
`json-crawler` 0.1.0 next to it in `vendor/json-crawler`. It speaks YouTube Music's own web API
(search, albums, artists), which changes without notice, so the copy is ours to fix: when a
parse breaks, edit it here instead of waiting for a release.

**Changes from the published crate**
- `reqwest` comes from the workspace (0.12) instead of the crate's own 0.13, so the build has one
  copy of it. The crate only uses `Client`, `.query()`, `.json()` and response bodies, all of which
  0.12 has. The `reqwest` TLS features were dropped from its feature list; the workspace picks the TLS.
- `json-crawler` is a path dependency.
- The live integration tests and `Cargo.lock` were removed.

**When YouTube Music changes shape** a search or an album comes back as a parse error naming the
JSON path it expected. The paths are constants in `src/nav_consts.rs`; the parsers are in `src/parse/`.
Compare against a fresh response (`yt-dlp` is not involved here, only this crate) and fix the path.
