import { fileURLToPath } from 'node:url'

// All environment configuration is read here, in one place, so that the rest of
// the code does not have to think about defaults or type coercion.

function text(name, fallback) {
  const value = process.env[name]
  return value === undefined || value === '' ? fallback : value
}

function integer(name, fallback) {
  const parsed = Number.parseInt(process.env[name], 10)
  return Number.isFinite(parsed) ? parsed : fallback
}

// A positive integer. Zero and negatives are rejected because for every timeout and
// limit here they mean "off", and a timeout of 0 silently disables the thing it
// bounds rather than removing the bound.
function positive(name, fallback) {
  const parsed = Number.parseInt(process.env[name], 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function list(name, fallback) {
  return text(name, fallback)
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)
}

// Parses "anime:anilist,series:cinemeta+tvmaze" into
// { anime: ['anilist'], series: ['cinemeta', 'tvmaze'] }.
//
// Types are separated by commas, so the providers within one type are separated by
// "+". A type may name several, most-preferred first, and the next answers when the
// first returns nothing. An entry without a colon is ignored rather than silently
// becoming a provider name that matches nothing.
function providerMap(name, fallback) {
  const map = {}
  for (const entry of list(name, fallback)) {
    const separator = entry.indexOf(':')
    if (separator === -1) continue
    const type = entry.slice(0, separator).trim()
    const names = entry
      .slice(separator + 1)
      .split('+')
      .map((value) => value.trim())
      .filter(Boolean)
    if (type && names.length > 0) map[type] = names
  }
  return map
}

export const config = {
  port: integer('PORT', 9000),

  // Absolute base URL of this server. Stream URLs handed to the browser are built
  // from it, so it must be the public address in production.
  publicUrl: text('PUBLIC_URL', `http://localhost:${integer('PORT', 9000)}`),

  // Which metadata provider to use for each content type, as "type:provider" pairs.
  // See providers/index.js for the available keys. A provider that does not declare
  // the requested type is not selectable, so a typo here is reported at boot rather
  // than answered with the wrong catalogue.
  //
  //   PROVIDERS=anime:anilist,series:cinemeta+tvmaze,movie:cinemeta+yts
  //
  // A type may list several providers separated by "+", most-preferred first. The
  // next one answers when the first returns nothing, which is what makes a provider
  // outage a degraded row rather than an empty one. Cinemeta is preferred because it
  // is a catalogue (every title, released or not) while YTS is a torrent tracker
  // (only what has been ripped).
  //
  // METADATA_PROVIDER overrides the anime entry, so the existing single-value
  // setting keeps working.
  providers: providerMap(
    'PROVIDERS',
    `anime:${text('METADATA_PROVIDER', 'anilist')},series:cinemeta+tvmaze,movie:cinemeta+yts`,
  ),

  // Which torrent indexers to search. See indexers/index.js for the available keys.
  //
  // Each indexer declares the content types it serves, so one list covers all of
  // them and the search only asks the relevant ones.
  //
  // AnimeTosho and apibay are deliberately absent. AnimeTosho's own shutdown page
  // says ingestion stopped in May 2026, so it is a frozen archive with nothing for a
  // currently airing show. apibay answers 403 from a datacenter IP and reports
  // "ok" while returning nothing. Both files are kept so either can be re-enabled
  // with this one variable.
  indexers: list('TORRENT_INDEXERS', 'subsplease,nyaa,knaben,torrents-csv,torrentio,yts,eztv'),

  // How long one indexer request may take before it is abandoned.
  //
  // A search fans out over many requests, so without this a single hung mirror holds
  // the whole response open. Measured against these hosts, the slowest real request is
  // ~3.5s (nyaa, which tries a blocked mirror before its working one), so this is set
  // above that: it exists to bound a hung request, not to truncate a slow answer.
  indexerTimeoutMs: positive('INDEXER_TIMEOUT_MS', 5000),

  // Nyaa mirrors move around, and the canonical nyaa.si is blocked by many
  // networks (it answers 503 through a filtering proxy). The list is tried in
  // order and the first one that returns results wins.
  nyaaBaseUrls: list('NYAA_BASE_URLS', 'https://nyaa.si,https://nyaa.net'),

  // Public trackers appended to every magnet link. Many indexers return torrents
  // with no working announce list, and without trackers we would never find peers.
  trackers: list('TORRENT_TRACKERS', [
    'udp://tracker.opentrackr.org:1337/announce',
    'udp://open.stealth.si:80/announce',
    'udp://tracker.torrent.eu.org:451/announce',
    'udp://exodus.desync.com:6969/announce',
  ].join(',')),

  // Where webtorrent stores in-progress downloads. Cleared on startup.
  // Relative paths are resolved against the server directory, not the current
  // working directory, so the cache does not move depending on where it is started.
  cacheDir: text('CACHE_DIR', fileURLToPath(new URL('../.torrent-cache', import.meta.url))),

  // How long an idle torrent is kept alive before its download is deleted.
  torrentTtlMs: integer('TORRENT_TTL_MINUTES', 20) * 60 * 1000,

  // Upper bound on simultaneously active torrents. Each one holds open sockets
  // and disk, so this must be bounded on a small server.
  maxTorrents: integer('MAX_TORRENTS', 12),

  // Upper bound on total bytes held on disk under CACHE_DIR - the torrent cache and the
  // HLS output together, because both are scratch data on the same disk.
  //
  // maxTorrents alone is not a disk limit: a 1080p episode is roughly 1.4GB, and an HLS
  // conversion reads the whole file, so twelve of them is well past 15GB. Torrents and
  // HLS sessions are evicted oldest-first once the budget is reached.
  maxCacheBytes: integer('MAX_CACHE_BYTES', 8 * 1024 ** 3),

  // How long to wait for a magnet to find peers and report its file list. This is
  // the slowest step in practice, and a swarm with a real seeder count can take
  // longer than half a minute to connect, so the cap is deliberately generous. It
  // is still clamped by whatever is left of the overall resolve deadline.
  metadataTimeoutMs: positive('METADATA_TIMEOUT_MS', 45000),

  // How long to wait for ffprobe to read the container header.
  probeTimeoutMs: positive('PROBE_TIMEOUT_MS', 20000),

  // Bounds for HLS sessions, which are what non-playable containers (MKV) are converted
  // into. Each session is one ffmpeg process and a directory holding the whole episode,
  // so both are capped: three concurrent episodes is roughly 4GB on a 40GB disk.
  //
  // A session is destroyed once it has been idle this long. It has to outlive the player
  // fetching segments, and a paused player stops fetching, so this is a grace period
  // rather than a per-request timeout.
  hlsSessionTtlMs: positive('HLS_SESSION_TTL_MINUTES', 30) * 60 * 1000,
  maxHlsSessions: positive('MAX_HLS_SESSIONS', 3),

  // How long to wait for ffmpeg to write the first playlist. Measured at under 3s for the
  // first segment, so this only bounds a transcode that never starts.
  hlsStartTimeoutMs: positive('HLS_START_TIMEOUT_MS', 30000),

  // How long a request for a segment that ffmpeg has not written yet may wait. The player
  // asks for the segment after the one it is playing, so a short wait is normal.
  hlsSegmentTimeoutMs: positive('HLS_SEGMENT_TIMEOUT_MS', 30000),

  // How many candidate releases to try before giving up on an episode.
  maxCandidates: integer('MAX_CANDIDATES', 3),

// Upper bound on resolving one episode or film, across all candidates. Without
  // this a run of unreachable swarms could keep a request open for minutes
  // (maxCandidates * (metadata + header)), holding a torrent slot and leaving the
  // viewer on a spinner.
  resolveDeadlineMs: positive('RESOLVE_DEADLINE_MS', 75000),

  // Hosts a torrent *indexer* may be reached at. Only knaben and YTS are
  // configurable, because both have already moved once: knaben is one operator's
  // service and YTS is mid-migration from yts.mx to movies-api.accel.li. Changing a
  // host is then an environment change rather than a rebuild.
  knabenBaseUrl: text('KNABEN_BASE_URL', 'https://api.knaben.org/v1'),
  ytsBaseUrl: text('YTS_BASE_URL', 'https://movies-api.accel.li/api/v2'),
  eztvBaseUrl: text('EZTV_BASE_URL', 'https://eztv.wf/api'),

  // torrents-csv is a single community instance (self-hostable, from Codeberg). It is
  // configurable for the same reason the others are: one operator's service can move
  // or disappear, and that should be an environment change rather than a rebuild.
  torrentsCsvBaseUrl: text('TORRENTS_CSV_BASE_URL', 'https://torrents-csv.com'),

  // Torrentio is the public Stremio indexer. It is the only source here that can be
  // searched by IMDb id with a season and episode, which is how it answers titles that
  // every text search misses. It is one operator's instance with no published rate
  // limit, so it is configurable and can be disabled by leaving it out of
  // TORRENT_INDEXERS.
  torrentioBaseUrl: text('TORRENTIO_BASE_URL', 'https://torrentio.strem.fun'),

  // TVMaze is keyless. TMDB is not, and is optional: when its key is absent the
  // movie provider stays YTS, which needs no account.
  tvmazeBaseUrl: text('TVMAZE_BASE_URL', 'https://api.tvmaze.com'),
  tmdbApiKey: text('TMDB_API_KEY', ''),

  // Cinemeta, the catalogue Stremio itself uses. One base URL covers meta and
  // catalogs because the service redirects between its hosts and axios follows.
  // Configurable because it has moved before, as YTS and knaben have.
  cinemetaBaseUrl: text('CINEMETA_BASE_URL', 'https://v3-cinemeta.strem.io'),

  // The Internet Archive, used as a last-resort stream source. It is the only place a
  // regional series can be found when every torrent indexer has nothing, and it serves
  // its own h264 derivative over HTTPS with range support rather than a swarm.
  // Configurable for the same reason the others are.
  archiveOrgBaseUrl: text('ARCHIVE_ORG_BASE_URL', 'https://archive.org'),

  // Where the video bytes come from, in order of preference. See
  // graphql/stream/sources/index.js for the available keys.
  //
  // The first source that returns a playable result wins, so listing two gives a
  // fallback: `debrid,torrent` serves from a CDN when the episode is cached there
  // and relays it otherwise.
  streamSources: list('STREAM_SOURCES', text('STREAM_SOURCE', 'torrent')),

  // TorBox API key. The debrid source is skipped entirely when this is unset, which
  // is what keeps it optional. It is a secret: it is only ever sent server-side.
  torboxToken: text('TORBOX_API_KEY', ''),

  // How long to wait for the debrid API. Both routes used here are fast, so this
  // only bounds an unreachable service.
  debridTimeoutMs: positive('DEBRID_TIMEOUT_MS', 15000),

  // Hosts a debrid download link is allowed to point at. The link comes from the
  // debrid API rather than from a request, but this server fetches it on the
  // viewer's behalf, so it is allowlisted rather than trusted. An entry matches that
  // host and any subdomain of it.
  //
  // The real host is a CDN node under one of these domains, e.g.
  // `nexus-144.enam.tb-cdn.io`. Both suffixes are listed because the provider has
  // rotated between them: `.pw` was verified against the live API first, and `.io`
  // replaced it. The obvious-looking `torbox.app` is NOT it - the API host and the
  // CDN host are different domains, so allowlisting the API host rejects every link.
  //
  // This is the one setting most likely to need changing when the provider rotates
  // its CDN domain again. A rejected link is logged with its hostname, so the new
  // value can be read off the log and set via DEBRID_CDN_HOSTS without a rebuild.
  debridCdnHosts: list('DEBRID_CDN_HOSTS', 'tb-cdn.pw,tb-cdn.io'),
}
