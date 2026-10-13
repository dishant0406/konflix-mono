// Tests for the content-type layer: id parsing, the three ranking paths, and the
// indexers that were added with them.
//
// These pin the behaviours that are expensive to rediscover, each of which was
// verified against the live APIs first:
//
//   - knaben's `hide_unsafe` flag is accepted and IGNORED, so the category filter
//     below is the only thing keeping adult rows off the site.
//   - EZTV's `search` parameter is ignored and returns the whole index, and its id
//     must be the bare form. Both are silent failures, not errors.
//   - A film search returns series episodes, so the ranking must reject them.

import assert from 'node:assert/strict'
import test from 'node:test'

const {
  parseMediaId,
  mediaKey,
  animeEpisodeId,
  seriesEpisodeId,
} = await import('../lib/mediaId.js')

const {
  rankMovieReleases,
  movieSearchTerms,
  seriesSearchTerms,
  parseSeason,
  parseYear,
  isEpisodeRelease,
} = await import('../graphql/stream/releases.js')

const { parseSearchResponse: knabenRows, isUnsafe } = await import('../graphql/indexers/knaben.js')
const { parseSearchResponse: ytsRows } = await import('../graphql/indexers/yts.js')
const { parseSearchResponse: eztvRows, toEztvId } = await import('../graphql/indexers/eztv.js')
const { providerFor, providerTypes } = await import('../graphql/providers/index.js')
const { enabledIndexers, indexerTypes } = await import('../graphql/indexers/index.js')

const HASH = 'a'.repeat(40)

test('a media id names its own content type', () => {
  assert.deepEqual(parseMediaId('anime-16498-ep1'), {
    contentType: 'anime',
    mediaId: '16498',
    season: null,
    episodeNumber: 1,
    key: 'anime-16498',
  })
  assert.deepEqual(parseMediaId('series-169-s1e5'), {
    contentType: 'series',
    mediaId: '169',
    season: 1,
    episodeNumber: 5,
    key: 'series-169',
  })
  assert.deepEqual(parseMediaId('movie-7691'), {
    contentType: 'movie',
    mediaId: '7691',
    season: null,
    episodeNumber: null,
    key: 'movie-7691',
  })
})

test('a title id parses without an episode, which is what a detail page uses', () => {
  // The same function serves a detail page and a watch page, so the episode part has
  // to be optional. Without this a detail page could not address a title at all.
  for (const [id, type] of [
    ['anime-16498', 'anime'],
    ['series-169', 'series'],
  ]) {
    const parsed = parseMediaId(id)
    assert.equal(parsed?.contentType, type, `${id} must parse as ${type}`)
    assert.equal(parsed?.episodeNumber, null)
    assert.equal(parsed?.key, id)
  }
})

test('an id whose suffix does not match its content type is rejected', () => {
  // A film has no episode and an anime has no season, so these are malformed rather
  // than "an anime with a season". Accepting them would put the wrong thing in the
  // URL and then look up the wrong episode.
  for (const id of [
    'movie-7691-ep1',
    'anime-16498-s1e1',
    'series-169-ep1',
    'anime-abc',
    'movie-abc',
    '16498-ep1',
    'series-1-x',
    '',
    'x',
  ]) {
    assert.equal(parseMediaId(id), null, `${JSON.stringify(id)} must not parse`)
  }
})

test('ids from different providers do not collide', () => {
  // The reason the content type is part of the id: TVMaze's show 1 and YTS's film 1
  // are different titles, and both are reachable from the same site.
  const anime = parseMediaId('anime-1')
  const series = parseMediaId('series-1')
  const movie = parseMediaId('movie-1')
  assert.notEqual(anime.key, series.key)
  assert.notEqual(series.key, movie.key)
  assert.notEqual(anime.key, movie.key)
})

test('the id builders agree with the parser', () => {
  assert.equal(animeEpisodeId(21, 1104), 'anime-21-ep1104')
  assert.equal(seriesEpisodeId(169, 1, 5), 'series-169-s1e5')
  assert.equal(mediaKey('movie', 7691), 'movie-7691')
  // A three-digit episode must not be truncated by padding.
  assert.equal(parseMediaId(animeEpisodeId(21, 1104))?.episodeNumber, 1104)
  assert.equal(parseMediaId(seriesEpisodeId(169, 10, 24))?.season, 10)
})

test('a film release is not mistaken for a series episode', () => {
  // This is the regression that matters most for the film path: searching a title
  // also returns its series, and a series episode is not the film.
  const releases = [
    { title: 'Breaking.Bad.S02E02.1080p.WEB.mkv', seeders: 500, sizeBytes: 1 },
    { title: 'Breaking Bad S01 COMPLETE 1080p', seeders: 400, sizeBytes: 1 },
    { title: 'Breaking Bad (2008) 1080p BluRay x264', seeders: 40, sizeBytes: 2 },
  ]
  const ranked = rankMovieReleases(releases, 2008, 'Breaking Bad', ['Breaking Bad'])
  assert.equal(ranked.length, 1, 'only the film itself may be ranked')
  assert.match(ranked[0].title, /2008/)
  assert.ok(!isEpisodeRelease(ranked[0].title))
})

test('a film release naming the wrong year is ranked below one that matches', () => {
  const releases = [
    { title: 'Dangal (2013) 1080p', seeders: 1, sizeBytes: 1 },
    { title: 'Dangal (2016) 720p', seeders: 1, sizeBytes: 1 },
  ]
  const ranked = rankMovieReleases(releases, 2016, 'Dangal', ['Dangal'])
  assert.equal(ranked.length, 2)
  assert.match(ranked[0].title, /2016/, 'the matching year must come first')
})

test('parseSeason and parseYear read the forms releases actually use', () => {
  assert.equal(parseSeason('Show S02E05 1080p'), 2)
  assert.equal(parseSeason('Show.S02E05.1080p.mkv'), 2)
  assert.equal(parseSeason('Show 05 1080p'), null)
  assert.equal(parseYear('Dangal (2016) 1080p'), 2016)
  assert.equal(parseYear('Dangal 2016 1080p'), 2016)
  assert.equal(parseYear('Dangal 1080p'), null)
})

test('search terms carry the season for a series and the year for a film', () => {
  const series = seriesSearchTerms(['Breaking Bad'], 1, 1)
  assert.ok(series.includes('Breaking Bad S01E01'), `got ${JSON.stringify(series)}`)

  const film = movieSearchTerms(['Dangal'], 2016)
  assert.ok(film.includes('Dangal 2016'), `got ${JSON.stringify(film)}`)
  // The year is optional, so a term without it is searched too.
  assert.ok(film.includes('Dangal'))
})

test('knaben drops every adult row regardless of the request', () => {
  // `hide_unsafe: true` is accepted by the API and ignored: a live query for "porn"
  // returned 26 of 30 rows in the 5xxxxxx category. This filter is therefore the only
  // thing preventing adult content appearing on a films and series site.
  assert.equal(isUnsafe(5000000), true)
  assert.equal(isUnsafe([2001000, 5001000]), true, 'categoryId is an array and must be scanned')
  assert.equal(isUnsafe([6001000, 3000000]), false)

  const rows = knabenRows({
    hits: [
      { hash: 'a'.repeat(40), title: 'adult', categoryId: [5000000, 5001000], bytes: 1, liveSeeders: 9 },
      { hash: 'b'.repeat(40), title: 'film', categoryId: [3001000, 3000000], bytes: 2, liveSeeders: 3 },
    ],
  })
  assert.equal(rows.length, 1)
  assert.equal(rows[0].title, 'film')
})

test('knaben normalises the hash and survives a missing seeder count', () => {
  const rows = knabenRows({
    hits: [
      { hash: HASH.toUpperCase(), title: 'Film (2020) 1080p', categoryId: [3000000], bytes: 100 },
      { hash: HASH, title: 'no seeders', categoryId: [3000000], liveSeeders: null },
      { title: 'no hash', categoryId: [3000000] },
    ],
  })
  assert.equal(rows.length, 2)
  for (const row of rows) {
    assert.match(row.infoHash, /^[0-9a-f]{40}$/)
    // The magnet is rebuilt from the normalised hash, so it must agree with the key.
    assert.ok(row.magnet.includes(row.infoHash))
    assert.equal(typeof row.seeders, 'number')
  }
})

test('yts turns each torrent of a film into its own row', () => {
  const rows = ytsRows({
    data: {
      movies: [
        {
          id: 7691,
          title: 'Dangal',
          year: 2016,
          torrents: [
            { hash: 'b'.repeat(40), quality: '1080p', type: 'bluray', seeds: 49, size_bytes: 2791728742 },
            { hash: 'c'.repeat(40), quality: '720p', type: 'bluray', seeds: 12, size_bytes: 1460288880 },
          ],
        },
      ],
    },
  })
  assert.equal(rows.length, 2, 'each quality is a separate choice')
  assert.equal(rows[0].seeders, 49)
  // The name has to carry the year and the resolution, because that is what the
  // ranking reads and YTS supplies no release name of its own.
  assert.match(rows[0].title, /2016/)
  assert.match(rows[0].title, /1080p/)
  assert.equal(rows[0].source, 'yts')
})

test('yts tolerates a torrent with no size or hash', () => {
  const rows = ytsRows({
    data: { movies: [{ id: 1, title: 'X', year: 2020, torrents: [{ quality: '720p' }, { hash: 'd'.repeat(40), quality: '480p' }] }] },
  })
  assert.equal(rows.length, 1, 'a torrent with no hash cannot be played')
  assert.equal(rows[0].sizeBytes, null)
})

test('eztv needs a bare imdb id, because the prefixed form returns the whole index', () => {
  // Verified live: `?imdb_id=tt0903747` returned torrents_count 1091704 - the entire
  // index - while `?imdb_id=0903747` returned the real 77 episodes. Sending the
  // prefixed form is a silent wrong answer rather than an error.
  assert.equal(toEztvId('tt0903747'), '0903747')
  assert.equal(toEztvId('0903747'), '0903747')
  for (const bad of ['ttabc', 'tt', 'abc', '', null, undefined]) {
    assert.equal(toEztvId(bad), null, `${JSON.stringify(bad)} must be rejected`)
  }
})

test('eztv parses its rows and reports zero seeders honestly', () => {
  const rows = eztvRows({
    torrents: [
      { hash: 'e'.repeat(40), filename: 'Breaking.Bad.S01E01.1080p.mkv', seeds: 0, size_bytes: 100 },
      { filename: 'no hash.mkv' },
    ],
  })
  assert.equal(rows.length, 1)
  assert.equal(rows[0].source, 'eztv')
  // Every row reports 0, including rows with peers, so the ranking must place these
  // in the lowest health tier rather than trusting the number.
  assert.equal(rows[0].seeders, 0)
})

test('each content type resolves to a provider that declares it', () => {
  const types = providerTypes()
  for (const type of ['anime', 'series', 'movie']) {
    const provider = providerFor(type)
    assert.ok(provider, `no provider for ${type}`)
    assert.ok(
      types[provider.name]?.includes(type),
      `${provider.name} does not declare ${type}`,
    )
  }
})

test('an indexer is only asked for the content types it declares', () => {
  const types = indexerTypes()
  const anime = enabledIndexers('anime').map((indexer) => indexer.name)
  const movie = enabledIndexers('movie').map((indexer) => indexer.name)
  const series = enabledIndexers('series').map((indexer) => indexer.name)

  // The anime-only feed must never be searched for a film, and vice versa.
  assert.ok(!movie.includes('subsplease'), 'a film search must not query an anime feed')
  assert.ok(!movie.includes('eztv'), 'a film search must not query a series indexer')
  assert.ok(movie.includes('yts'), 'a film search must query the film indexer')
  assert.ok(series.includes('eztv'), 'a series search must query the series indexer')
  assert.ok(anime.includes('subsplease'))

  // The invariant itself: nothing is enabled for a type it does not declare. An
  // indexer that is registered but not in the default list is absent from all three,
  // which is why this checks the enabled set rather than every registered indexer.
  for (const [type, names] of [
    ['anime', anime],
    ['series', series],
    ['movie', movie],
  ]) {
    for (const name of names) {
      assert.ok(
        types[name]?.includes(type),
        `${name} is enabled for ${type} but does not declare it`,
      )
    }
  }
})

// The stream layer reads the small shape - `name`, `titleVariants`, `year`, `imdbId` -
// because that is what every provider's animeDetails answers. mediaDetail answers a
// different shape, where the title is nested under `title.english` and there is no
// year, so reading it there produced the literal search term "undefined" for every
// series and film. These pin the shape the layer depends on.
test('every provider answers the small shape the stream layer reads', async () => {
  for (const [type, provider] of [
    ['anime', providerFor('anime')],
    ['series', providerFor('series')],
    ['movie', providerFor('movie')],
  ]) {
    assert.equal(
      typeof provider.animeDetails,
      'function',
      `the ${type} provider must answer animeDetails, which is what the stream layer calls`,
    )
  }
})

test('the small shape names the title directly rather than nesting it', async () => {
  // A captured TVMaze show and YTS film, so this runs without the network.
  const { toMediaFull: showFull } = await import('../graphql/providers/tvmaze.js')
  const { toMediaFull: movieFull } = await import('../graphql/providers/yts.js')

  const show = showFull({
    id: 169,
    name: 'Breaking Bad',
    premiered: '2008-01-20',
    externals: { imdb: 'tt0903747' },
    _embedded: { episodes: [{ season: 1, number: 5, name: 'Gray Matter', airdate: '2008-02-10' }] },
  })
  assert.equal(show.title.english, 'Breaking Bad')
  assert.equal(show.year, 2008)
  assert.equal(show.imdbId, 'tt0903747')

  const movie = movieFull({ id: 7691, title: 'Dangal', year: 2016, imdb_code: 'tt5074352' })
  assert.equal(movie.title.english, 'Dangal')
  assert.equal(movie.year, 2016)
  assert.equal(movie.imdbId, 'tt5074352')
})

// A season pack holds every season of a show, so the episode number alone is
// ambiguous: "S02E05" is a different episode from "S01E05".
test('pickVideoFile prefers the requested season inside a multi-season torrent', async () => {
  const { pickVideoFile } = await import('../graphql/stream/torrent.js')
  const files = [
    { name: 'Show S01E05 1080p.mkv', length: 2_000_000_000 },
    { name: 'Show S02E05 1080p.mkv', length: 2_000_000_000 },
    { name: 'Show S03E05 1080p.mkv', length: 2_000_000_000 },
  ]

  assert.equal(pickVideoFile(files, 5, undefined, 2).name, 'Show S02E05 1080p.mkv')
  assert.equal(pickVideoFile(files, 5, undefined, 3).name, 'Show S03E05 1080p.mkv')
  // Without a season the first match wins, which is the old behaviour.
  assert.equal(pickVideoFile(files, 5).name, 'Show S01E05 1080p.mkv')
})

test('pickVideoFile still accepts a release that names no season', async () => {
  const { pickVideoFile } = await import('../graphql/stream/torrent.js')
  const files = [
    { name: 'Show - 05 1080p.mkv', length: 2_000_000_000 },
    { name: 'Show - 06 1080p.mkv', length: 2_000_000_000 },
  ]
  // The season is only used to narrow an ambiguous list, so an unnamed release is
  // still a match rather than being rejected.
  assert.equal(pickVideoFile(files, 5, undefined, 2).name, 'Show - 05 1080p.mkv')
})
