// Cinemeta provider, and the provider fallback chain.
//
// Every mapping here is a pure function over a captured response, so these run
// without the network. The shapes are copied from real answers from the live service.

import test from 'node:test'
import assert from 'node:assert/strict'

import {
  isReleased,
  parseReleaseInfo,
  parseRuntime,
  toEpisodeList,
  toMediaFull,
  toMediaList,
  usableMetas,
} from '../graphql/providers/cinemeta.js'
import { emptyFor, isEmpty, providerFor, providersFor, withFallback } from '../graphql/providers/index.js'

// A captured catalog entry, trimmed to the fields the mapping reads.
const catalogEntry = {
  id: 'tt27548557',
  imdb_id: 'tt27548557',
  type: 'movie',
  name: 'Dhamaal 4',
  poster: 'https://live.metahub.space/poster/small/tt27548557/img',
  background: 'https://live.metahub.space/background/medium/tt27548557/img',
  releaseInfo: '2026',
  released: '2026-06-06T00:00:00.000Z',
  runtime: '143 min',
  imdbRating: '6.7',
  genres: ['Comedy'],
  cast: ['Ajay Devgn'],
  popularity: 8.65,
}

// A captured meta response, with the fields a film carries.
const movieMeta = {
  ...catalogEntry,
  description: 'The Dhamaal boys are back for another treasure hunt.',
  trailers: [{ source: 'Mf8Yiy3FL24', type: 'Trailer' }],
  credits_cast: [
    { character: 'Guddu', name: 'Ajay Devgn', profile_path: '/vnHQQFzTjJ0.jpg', id: 42803 },
    { character: 'Adi', name: 'Arshad Warsi', profile_path: null, id: 85889 },
  ],
}

// A captured series meta response. Season 0 is present on purpose: it holds specials
// and must not be counted as an episode.
const seriesMeta = {
  id: 'tt0903747',
  imdb_id: 'tt0903747',
  type: 'series',
  name: 'Breaking Bad',
  poster: 'https://images.metahub.space/poster/small/tt0903747/img',
  background: 'https://images.metahub.space/background/medium/tt0903747/img',
  releaseInfo: '2008–2013',
  released: '2008-01-20T00:00:00.000Z',
  status: 'Ended',
  runtime: '49 min',
  imdbRating: '9.5',
  genres: ['Crime', 'Drama'],
  trailers: [{ source: 'VFkjBy2b50Q', type: 'Trailer' }],
  videos: [
    { id: 'tt0903747:0:1', title: 'Good Cop / Bad Cop', season: 0, number: 1, overview: 'A minisode.' },
    { id: 'tt0903747:1:1', title: 'Pilot', season: 1, number: 1, released: '2008-01-21T05:00:00.000Z', overview: 'x' },
    { id: 'tt0903747:1:2', title: 'Cat in the Bag', season: 1, number: 2, released: '2008-01-27T05:00:00.000Z' },
    { id: 'tt0903747:5:16', title: 'Felina', season: 5, number: 16, released: '2013-09-29T05:00:00.000Z' },
  ],
}

test('a release range is read as its first year, and a single year as itself', () => {
  // A series reports "2008–2013" and a film reports "2026". Only the first year is
  // wanted, because that is the year a release name carries.
  assert.equal(parseReleaseInfo('2008–2013'), 2008)
  assert.equal(parseReleaseInfo('2026'), 2026)
  assert.equal(parseReleaseInfo(null), null)
  assert.equal(parseReleaseInfo(''), null)
  // An announcement can carry a dash as a plain hyphen.
  assert.equal(parseReleaseInfo('2019-2021'), 2019)
})

test('a runtime in minutes is read as a number', () => {
  assert.equal(parseRuntime('143 min'), 143)
  assert.equal(parseRuntime('49 min'), 49)
  assert.equal(parseRuntime(null), null)
})

test('a film catalog entry maps onto the card shape', () => {
  const row = toMediaList(catalogEntry)

  // The id is self-describing and carries the IMDb id, which is what the frontend
  // routes on and what EZTV is searched with.
  assert.equal(row.id, 'movie-tt27548557')
  // Cinemeta has one name and it goes in `english`, the field the UI prefers and the
  // stream layer searches with.
  assert.equal(row.title.english, 'Dhamaal 4')
  assert.equal(row.seasonYear, 2026)
  assert.equal(row.duration, 143)
  // Cinemeta scores out of 10 and AniList out of 100, so they are normalised or the
  // two sites show incomparable numbers.
  assert.equal(row.averageScore, 67)
  assert.deepEqual(row.genres, ['Comedy'])
  // The wide image is the hero banner, and the poster is what a card shows.
  assert.match(row.bannerImage, /background/)
  assert.match(row.coverImage.extraLarge, /poster/)
  // A film has no episode count in a catalog entry.
  assert.equal(row.episodes, null)
  assert.equal(row.isAdult, false)
})

test('a series catalog entry maps with its status and a date range', () => {
  const row = toMediaList(seriesMeta)
  assert.equal(row.id, 'series-tt0903747')
  assert.equal(row.format, 'TV')
  assert.equal(row.status, 'FINISHED')
  assert.equal(row.startDate.year, 2008)
  // The end of the range is what "Ended" means, and it is the only place it appears.
  assert.equal(row.endDate.year, 2013)
})

test('an announced title is kept out of every row, and a released one is not', () => {
  const now = Date.parse('2026-10-13T00:00:00.000Z')

  assert.equal(isReleased({ released: '2026-06-06T00:00:00.000Z', releaseInfo: '2026' }, now), true)
  // Announced for December, not out in October.
  assert.equal(isReleased({ released: '2026-12-02T00:00:00.000Z', releaseInfo: '2026' }, now), false)
  // No exact date, so the year is the only signal. A future year is excluded.
  assert.equal(isReleased({ releaseInfo: '2027' }, now), false)
  assert.equal(isReleased({ releaseInfo: '2025' }, now), true)
  // Neither field: an old catalog entry with no dates must not be dropped.
  assert.equal(isReleased({}, now), true)
})

test('specials are not counted as episodes', () => {
  // Breaking Bad carries nine season-0 minisodes. Counting them reported 71 episodes
  // for a 62-episode show, which the watch page uses as its total.
  const episodes = toEpisodeList(seriesMeta)
  assert.equal(episodes.length, 3)
  assert.ok(episodes.every((episode) => episode.season >= 1))
  // The id is already self-describing, so the watch page routes from it alone.
  assert.equal(episodes[0].episodeId, 'series-tt0903747-s1e1')
  assert.equal(episodes[2].episodeId, 'series-tt0903747-s5e16')
})

test('a film detail carries the cast, the trailer and the year the stream layer reads', () => {
  const film = toMediaFull(movieMeta)
  assert.equal(film.description, 'The Dhamaal boys are back for another treasure hunt.')
  assert.deepEqual(film.trailer, { id: 'Mf8Yiy3FL24', site: 'YouTube' })
  assert.equal(film.characters.edges.length, 2)
  assert.equal(film.characters.edges[0].role, 'Guddu')
  assert.equal(film.characters.edges[0].node.name.full, 'Ajay Devgn')
  assert.equal(film.externalLinks[0].url, 'https://www.imdb.com/title/tt27548557/')
  // Read by resolveStream, not rendered: the year separates a film from its remakes
  // and the IMDb id is how the series indexer is searched.
  assert.equal(film.year, 2026)
  assert.equal(film.imdbId, 'tt27548557')
})

test('a series detail reports its episode list and count', () => {
  const show = toMediaFull(seriesMeta)
  assert.equal(show.episodeList.length, 3)
  assert.equal(show.episodes, 3)
  assert.equal(show.imdbId, 'tt0903747')
})

test('a cast list with no character names still produces credits', () => {
  // `cast` carries names only; `credits_cast` carries the character and a photo. Both
  // are mapped so a title with only one of them still shows a cast.
  const film = toMediaFull({ ...catalogEntry, credits_cast: undefined, cast: ['Ajay Devgn', 'Arshad Warsi'] })
  assert.equal(film.characters.edges.length, 2)
  assert.equal(film.characters.edges[0].role, null)
  assert.equal(film.characters.edges[0].node.name.full, 'Ajay Devgn')
})

test('a provider chain is ordered, and the first that answers wins', async () => {
  // The configured chain for a film is Cinemeta then YTS, so a title Cinemeta does
  // not know is still looked up rather than reported missing.
  const chain = providersFor('movie').map((provider) => provider.name)
  assert.equal(chain[0], 'cinemeta')
  assert.ok(chain.includes('yts'), 'a film must have a fallback provider')
  // The series chain keeps TVMaze reachable, which is where Indian television
  // coverage comes from.
  assert.deepEqual(
    providersFor('series').map((provider) => provider.name),
    ['cinemeta', 'tvmaze'],
  )
})

test('a search no provider can answer returns an empty page, never null', async () => {
  // The frontend reads `results` off the response, so null would be a render crash
  // rather than an empty row.
  const page = await withFallback('movie', 'mediaSearch', [{ term: 'zzzqqqnotathingatall' }])
  assert.ok(page, 'a search must answer with a page')
  assert.ok(Array.isArray(page.results), 'a page must carry a results array')
  assert.equal(page.hasNextPage, false)
})

test('a detail no provider can answer returns null rather than throwing', async () => {
  assert.equal(await withFallback('movie', 'mediaDetail', ['not-an-id', 'movie']), null)
  // A well-formed id no provider holds. Every provider rejects it by regex before
  // making a request, so this stays offline.
  assert.equal(await withFallback('series', 'mediaDetail', ['tt0000000', 'series']), null)
})

test('a page with no results is an empty answer, not an answer', () => {
  // A page whose `results` is null must not stop the chain, or one provider returning
  // a degenerate page would silently disable the fallback.
  assert.equal(isEmpty({ results: null }), true)
  assert.equal(isEmpty({ results: [] }), true)
  assert.equal(isEmpty({ results: [{ id: 'x' }] }), false)
  // A detail record with no id cannot be linked to, so it is not an answer either.
  assert.equal(isEmpty({ id: null, title: { english: 'x' } }), true)
  assert.equal(isEmpty({ id: 'movie-tt1', title: { english: 'x' } }), false)
  assert.equal(isEmpty(null), true)
  assert.equal(isEmpty(undefined), true)
})

test('what a spent chain answers with depends on the kind of question', () => {
  // A list and a search must answer a page so the caller can render an empty row; a
  // detail must answer null so the caller can render a not-found page.
  assert.equal(emptyFor('mediaDetail'), null)
  for (const method of ['mediaList', 'mediaSearch']) {
    const page = emptyFor(method)
    assert.deepEqual(page.results, [])
    assert.equal(page.hasNextPage, false)
  }
})

test('a content type with nothing configured has no chain, rather than the wrong one', () => {
  // Answering a misconfigured type with the anime provider would render anime rows on
  // the films site - a silent wrong answer. An empty chain makes the caller answer with
  // nothing instead.
  assert.deepEqual(providersFor('nonsense'), [])
  // A request with no content type at all keeps its pre-content-types behaviour.
  assert.equal(providersFor(undefined)[0].name, 'anilist')
})

test('a provider is never selected for a type it does not declare', () => {
  // A film provider answering a series request would return the wrong catalogue,
  // which is worse than the default one.
  for (const [type, chain] of [
    ['movie', providersFor('movie')],
    ['series', providersFor('series')],
    ['anime', providersFor('anime')],
  ]) {
    for (const provider of chain) {
      assert.ok(
        provider.types.includes(type),
        `${provider.name} is selected for ${type} but does not declare it`,
      )
    }
  }
})

test('the anime accessor always has a provider, even with nothing configured', () => {
  // The anime root fields call providerFor directly and cannot handle undefined, so it
  // must answer something for any input. The chain itself is allowed to be empty.
  for (const type of ['anime', 'series', 'movie', 'nonsense', undefined]) {
    assert.ok(providerFor(type), `providerFor(${type}) must answer a provider`)
  }
})

test('a film search never reaches an anime feed, and vice versa', () => {
  // The property that keeps the two sites from showing each other's catalogue.
  const names = (type) => providersFor(type).map((provider) => provider.name)
  assert.ok(!names('movie').includes('anilist'), 'a film must not be looked up in AniList')
  assert.ok(!names('anime').includes('cinemeta'), 'an anime must not be looked up in Cinemeta')
})

test('an entry with no id is dropped rather than rendered as a dead card', () => {
  // An id-less entry would produce "movie-null", which parses under the media-id
  // grammar and becomes a routable URL that 404s rather than an obvious bug.
  assert.deepEqual(usableMetas([{ name: 'Ghost' }, { imdb_id: 'tt1', name: 'Real' }]).map((m) => m.name), ['Real'])
  assert.deepEqual(toEpisodeList({ videos: [{ season: 1, number: 1 }] }), [])
})
