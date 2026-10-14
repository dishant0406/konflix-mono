import test from 'node:test'
import assert from 'node:assert/strict'

import { parseEpisode, isBatch, isExtra, isLikelyMovie, matchesAnyTitle, parseResolution, parseCodec, parseAudio, scoreRelease, rankReleases, searchTerms, isVideoFile, isRelevant, matchesEpisode, parseSeason, seriesSearchTerms, titleVariants } from '../graphql/stream/releases.js'
import { titleOf } from '../graphql/indexers/index.js'

test('isVideoFile recognises video containers only', () => {
  assert.equal(isVideoFile('episode.mkv'), true)
  assert.equal(isVideoFile('episode.MP4'), true)
  assert.equal(isVideoFile('readme.txt'), false)
  assert.equal(isVideoFile('subs.ass'), false)
})

test('parseResolution reads common release formats', () => {
  assert.equal(parseResolution('[SubsPlease] Show - 01 (1080p)'), 1080)
  assert.equal(parseResolution('Show 720p WEB-DL'), 720)
  assert.equal(parseResolution('Show 480p'), 480)
  assert.equal(parseResolution('Show 2160p'), 2160)
  assert.equal(parseResolution('Show no resolution'), null)
})

test('parseResolution is not confused by digits after the p', () => {
  // "10bit" tags follow the resolution with no separator.
  assert.equal(parseResolution('Show 720p10bit'), 720)
  assert.equal(parseResolution('Show 1080p10bit x264'), 1080)
})

test('parseCodec detects hevc and h264', () => {
  assert.equal(parseCodec('Show 1080p x265 FLAC'), 'hevc')
  assert.equal(parseCodec('Show 1080p HEVC'), 'hevc')
  assert.equal(parseCodec('Show 1080p x264 AC3'), 'h264')
  assert.equal(parseCodec('Show 1080p H.264'), 'h264')
  assert.equal(parseCodec('Show 1080p AV1'), 'av1')
  assert.equal(parseCodec('Show 1080p'), null)
})

test('parseAudio separates lossless from compressed', () => {
  assert.equal(parseAudio('Show FLAC'), 'lossless')
  assert.equal(parseAudio('Show DTS-HD'), 'lossless')
  assert.equal(parseAudio('Show AAC2.0'), 'compressed')
  assert.equal(parseAudio('Show'), null)
})

test('parseEpisode reads episode numbers from several naming styles', () => {
  assert.equal(parseEpisode('[SubsPlease] Show - 12 (1080p)'), 12)
  assert.equal(parseEpisode('Show Episode 7'), 7)
  assert.equal(parseEpisode('Show E05'), 5)
  assert.equal(parseEpisode('Show EP.102'), 102)
  assert.equal(parseEpisode('Show 第12话'), 12)
})

test('parseEpisode returns null when the release names no episode', () => {
  // A resolution must never be mistaken for an episode number: "Show - 1080p" is
  // not episode 1080.
  assert.equal(parseEpisode('Show - 1080p'), null)
  assert.equal(parseEpisode('Show 1080p WEB-DL'), null)
  assert.equal(parseEpisode('NARUTO.S01.1080p.CR.WEB-DL.AAC2.0.H.264-Kitsune'), null)
})

test('isBatch identifies season packs and boxes', () => {
  assert.equal(isBatch('Show S01 1080p'), true)
  assert.equal(isBatch('Show Batch 1080p'), true)
  assert.equal(isBatch('Show - Box 05 [BDRip]'), true)
  assert.equal(isBatch('Show - 12 (1080p)'), false)
})

test('scoreRelease prefers 1080p h264 and penalises batches', () => {
  const right = scoreRelease({ title: '[SubsPlease] Show - 12 (1080p) [ABC]', seeders: 10 })
  const lowres = scoreRelease({ title: '[Group] Show - 12 (480p x264)', seeders: 1 })
  const batch = scoreRelease({ title: '[Group] Show S01 1080p x264', seeders: 99 })

  assert.ok(right > lowres, '1080p must beat 480p')
  assert.ok(right > batch, 'a single episode must beat a season pack')
})

test('matchesEpisode rejects a release for a different episode', () => {
  assert.equal(matchesEpisode('Show S01E12 1080p', 12), true)
  assert.equal(matchesEpisode('Show S01E11 1080p', 12), false)
  // A release that names no episode is allowed through; openRelease decides
  // whether it can be used safely.
  assert.equal(matchesEpisode('Show S01 1080p x264', 12), true)
})

test('rankReleases drops unplayable releases and orders the rest', () => {
  const ranked = rankReleases(
    [
      { title: 'Show - 12 (1080p x265 HEVC)', seeders: 500, infoHash: 'a' },
      { title: 'Show - 12 (1080p x264 AAC)', seeders: 5, infoHash: 'b' },
      { title: 'Show - 11 (1080p x264 AAC)', seeders: 50, infoHash: 'c' },
    ],
    12,
    'Show',
  )

  assert.deepEqual(
    ranked.map((release) => release.infoHash),
    ['b'],
    'the HEVC release and the wrong episode are both filtered out',
  )
})

test('searchTerms fans out over the ways releases write an episode number', () => {
  // Releases use "1", "01", "001" and "S01E01" interchangeably and the indexers
  // match literally, so one form finds a fraction of what exists. Measured live:
  // "Bocchi the Rock 1" ranked 0 usable releases while the full fan-out ranked 7.
  const terms = searchTerms(['Bocchi the Rock!'], 1)
  assert.deepEqual(terms, [
    'Bocchi the Rock 1',
    'Bocchi the Rock 01',
    'Bocchi the Rock 001',
    'Bocchi the Rock S01E01',
  ])
})

test('searchTerms strips punctuation and keeps CJK titles', () => {
  assert.deepEqual(searchTerms(['Kaguya-sama: Love is War'], 3), [
    'Kaguya-sama Love is War 3',
    'Kaguya-sama Love is War 03',
    'Kaguya-sama Love is War 003',
    'Kaguya-sama Love is War S01E03',
  ])
  // An ASCII-only \w would strip this title to nothing and search for the episode
  // number alone.
  assert.deepEqual(searchTerms(['進撃の巨人'], 1)[0], '進撃の巨人 1')
})

test('searchTerms does not truncate a long-running episode number', () => {
  // ONE PIECE is past episode 1100; padStart must leave a three-digit number alone.
  assert.deepEqual(searchTerms(['One Piece'], 1104), [
    'One Piece 1104',
    'One Piece S01E1104',
  ])
})

test('searchTerms covers every title variant and stays bounded', () => {
  const terms = searchTerms(['Attack on Titan', 'Shingeki no Kyojin'], 1)
  assert.ok(terms.includes('Attack on Titan 1'))
  assert.ok(terms.includes('Shingeki no Kyojin 1'))
  // Each term costs one request per indexer, so the list is capped.
  assert.ok(terms.length <= 8, `expected at most 8 terms, got ${terms.length}`)
})

test('searchTerms falls back to the bare title without an episode number', () => {
  assert.deepEqual(searchTerms(['Show'], null), ['Show'])
  assert.deepEqual(searchTerms([], 1), [])
})

test('parseEpisode handles the SxxExx naming used by WEB-DL releases', () => {
  assert.equal(parseEpisode('One Piece S01E1158 1080p CR WEB-DL AAC2.0 H 264-VARYG'), 1158)
  assert.equal(parseEpisode('Show S02E07 1080p'), 7)
})

test('isRelevant rejects a different show that merely contains the title', () => {
  // These all contain the searched title but are different shows.
  assert.equal(isRelevant('Boruto - Naruto Next Generations Box Set 1', 'Naruto'), false)
  assert.equal(isRelevant('One Piece Fan Letter S01E01', 'One Piece'), true)
  assert.equal(isRelevant('One Piece S01E1158 1080p CR WEB-DL', 'One Piece'), true)
})

test('isRelevant accepts the naming styles real indexers use', () => {
  assert.equal(isRelevant('[SubsPlease] One Piece - 1080 (1080p)', 'One Piece'), true)
  assert.equal(isRelevant('[Group] Naruto Shippuden 163 (BDRip 1080p)', 'Naruto Shippuden'), true)
  assert.equal(isRelevant('NARUTO.S01.1080p.CR.WEB-DL.H.264-Kitsune', 'Naruto'), true)
  assert.equal(isRelevant('[Naruto] Episode 1 [1080p]', 'Naruto'), true)
  assert.equal(isRelevant('Bleach S01E01 1080p', 'Naruto'), false)
})

test('rankReleases never returns a release for a different episode', () => {
  // Serving the wrong episode silently is worse than serving nothing.
  const ranked = rankReleases(
    [{ title: 'Naruto S01E11 1080p x264 AAC', seeders: 500, infoHash: 'wrong' }],
    12,
    'Naruto',
  )
  assert.deepEqual(ranked, [])
})

test('rankReleases keeps the requested episode', () => {
  const ranked = rankReleases(
    [{ title: 'Naruto S01E12 1080p x264 AAC', seeders: 5, infoHash: 'right' }],
    12,
    'Naruto',
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['right'])
})

test('isExtra matches words separated by underscores', () => {
  // "_" is a regex word character, so a plain \bmovie\b never matched this title.
  assert.equal(isExtra('[DB]_Naruto_Shippuuden_Movie_[75F57621]'), true)
  assert.equal(isExtra('Show.S01E01.1080p.WEB-DL'), false)
})

test('isBatch treats an episode range as a batch', () => {
  assert.equal(isBatch('Naruto [1-162 of 220] [BDRip] [800p]'), true)
  assert.equal(isBatch('Naruto Shippuden 1-79 HD [Multi-Sub]'), true)
  assert.equal(isBatch('Show - 12 (1080p)'), false)
})

test('a sequel number after Movie is not an episode number', () => {
  assert.equal(parseEpisode('[Believe] Naruto Shippuden the Movie 1 (BD 1080p x264)'), null)
})

test('parseEpisode reads the bare episode number fansub releases use', () => {
  // "[HatSubs] One Piece 1104 (WEB 1080p)" previously parsed as "no episode", which
  // made episode 1104 an eligible fallback candidate for episode 1.
  assert.equal(parseEpisode('[HatSubs] One Piece 1104 (WEB 1080p) [F8FA71E1].mkv'), 1104)
  assert.equal(parseEpisode('[Group] Show 001 (1080p) [ABC].mkv'), 1)
})

test('parseEpisode is not fooled by audio channels or bit depths', () => {
  assert.equal(parseEpisode('Show 5.1 (Dual Audio) [1080p]'), null)
  assert.equal(parseEpisode('[Group] Show 720p10bit x264'), null)
})

test('parseEpisode reads the bare "Show NN Word" form', () => {
  // This is the shape documented at the top of releases.js, and it previously
  // parsed as null, which put the release in the fallback bucket where it could be
  // served for any episode.
  assert.equal(parseEpisode('Naruto Shippuden 163 Remaster (BDRip 1080p x265 FLAC Multi) - Ryujin'), 163)
  assert.equal(parseEpisode('One Piece 1000 WEB-DL 1080p AAC'), 1000)
  assert.equal(parseEpisode('Show 12 1080p x264'), 12)
  // Collections must still not read as an episode.
  assert.equal(parseEpisode('Show S01 1080p'), null)
  assert.equal(parseEpisode('Show Part 2 1080p'), null)
})

test('matchesAnyTitle accepts a release named with any known title variant', () => {
  // A show has several names and releases use them inconsistently. Checking only
  // the English title rejected every correct release of this show.
  const variants = ['Attack on Titan', 'Shingeki no Kyojin', '進撃の巨人']
  assert.equal(matchesAnyTitle('[SubsPlease] Shingeki no Kyojin - 01 (1080p)', variants), true)
  assert.equal(matchesAnyTitle('Attack on Titan S01E01 1080p', variants), true)
  assert.equal(matchesAnyTitle('進撃の巨人 S01E01 1080p', variants), true)
  assert.equal(matchesAnyTitle('Bleach S01E01 1080p', variants), false)
})

test('rankReleases uses every title variant', () => {
  const ranked = rankReleases(
    [{ title: '[SubsPlease] Shingeki no Kyojin - 01 (1080p)', seeders: 10, infoHash: 'romaji' }],
    1,
    'Attack on Titan',
    ['Attack on Titan', 'Shingeki no Kyojin'],
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['romaji'])
})

test('rankReleases enforces the episode number when no title is comparable', () => {
  // A CJK-only title cannot be matched against ASCII release names, so relevance
  // has to pass everything. The episode number is then the only signal available,
  // which means a wrong release for the SAME episode number cannot be detected.
  // That is a real limitation, and this test pins the behaviour that matters: a
  // release naming a different episode is still rejected.
  const ranked = rankReleases(
    [
      { title: 'Some Other Show S01E05 1080p x264', seeders: 99, infoHash: 'wrongepisode' },
      { title: '進撃の巨人 S01E01 1080p x264', seeders: 5, infoHash: 'right' },
    ],
    1,
    '進撃の巨人',
    ['進撃の巨人'],
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['right'])
})

test('matchesAnyTitle accepts everything when no title is comparable', () => {
  // Documented limitation: with only a CJK title there is nothing to gate on.
  assert.equal(matchesAnyTitle('Any Show 1 1080p', ['進撃の巨人']), true)
  assert.equal(matchesAnyTitle('Any Show 1 1080p', ['Attack on Titan']), false)
})

test('isLikelyMovie spots films named without the word movie', () => {
  // "One Piece - Film Red 2022" was ranked as a fallback for episode 1 because it
  // names no episode number.
  assert.equal(isLikelyMovie('One Piece: Baron Omatsuri and the Secret Island (2005) [1080p]'), true)
  assert.equal(isLikelyMovie('Naruto the Movie 1 (2004)'), true)
  assert.equal(isLikelyMovie('[SubsPlease] One Piece - 001 (1080p) [ABC]'), false)
  assert.equal(isLikelyMovie('NARUTO.S01.1080p.CR.WEB-DL.H.264'), false)
})

test('rankReleases never falls back to a film named without the word movie', () => {
  const ranked = rankReleases(
    [{ title: 'One Piece: Baron Omatsuri and the Secret Island (2005) [1080p]', seeders: 99, infoHash: 'film' }],
    1,
    'One Piece',
  )
  assert.deepEqual(ranked, [])
})

test('parseEpisode ignores part and season numbers', () => {
  // "One Piece - Season 01 Part 1 [1080p]" is a collection. Reading the trailing
  // "1" as episode 1 made a whole season look like the requested episode.
  assert.equal(parseEpisode('One Piece - Season 01 Part 1 [1080p] [x264] [Multi-Audio]'), null)
  assert.equal(parseEpisode('Show Part 2 [1080p]'), null)
  assert.equal(parseEpisode('Show Vol.1 [720p]'), null)
  assert.equal(parseEpisode('Show S01 [1080p]'), null)
  // A real episode number is still read.
  assert.equal(parseEpisode('One Piece - 001 [1080p]'), 1)
})

test('rankReleases keeps single-file fallbacks behind named episodes', () => {
  // A release that names no episode can still be the right file (many single-episode
  // torrents are simply named after the show), but it must never outrank a release
  // that names the episode.
  const ranked = rankReleases(
    [
      { title: 'Naruto [1080p x264]', seeders: 50, infoHash: 'unnamed' },
      { title: 'Naruto - 001 [1080p x264 AAC]', seeders: 2, infoHash: 'named' },
    ],
    1,
    'Naruto',
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['named', 'unnamed'])
})

test('isExtra recognises movies, OVAs and trailers', () => {
  assert.equal(isExtra('Naruto Shippuden - The Movie (BDRip 1080p x264)'), true)
  assert.equal(isExtra('NARUTO OVAs (1-11) + Special [English Subbed] 480p'), true)
  assert.equal(isExtra('Naruto - Coming to FUNimation! (1080p) [Official Trailer]'), true)
  assert.equal(isExtra('[SubsPlease] Naruto - 12 (1080p) [ABC]'), false)
})

test('rankReleases prefers a real episode over a movie of the same show', () => {
  // Searching "Naruto 1" surfaced "Naruto Shippuden - The Movie" first, because a
  // release that names no episode number still collected the episode bonus.
  const ranked = rankReleases(
    [
      { title: '[uP] Naruto Shippuden - The Movie (BDRip 1080p x264 AC3 Multi)', seeders: 40, infoHash: 'movie' },
      { title: '[Group] Naruto - 001 (1080p x264 AAC)', seeders: 5, infoHash: 'episode' },
    ],
    1,
    'Naruto',
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['episode'])
})

test('rankReleases does not fall back to a movie when no episode is named', () => {
  const ranked = rankReleases(
    [
      { title: 'Naruto the Movie - Ninja Clash in the Land of Snow (1080p x264)', seeders: 99, infoHash: 'movie' },
      { title: 'Naruto Movies + EXTRA 1080p', seeders: 99, infoHash: 'pack' },
    ],
    1,
    'Naruto',
  )
  assert.deepEqual(ranked, [])
})

test('rankReleases orders by stream health before quality', () => {
  // A 2-seeder release previously outranked a 379-seeder one for the same episode
  // because it named its codec and seeders were only a tie-break. Health decides
  // whether a release can be played at all, so it is the primary ordering.
  const ranked = rankReleases(
    [
      { title: 'Show - 12 (1080p x264 AAC)', seeders: 2, infoHash: 'barely' },
      { title: 'Show - 12 (1080p x265)', seeders: 379, infoHash: 'healthy' },
      { title: 'Show - 12 (1080p x264 AAC)', seeders: 40, infoHash: 'seeded' },
    ],
    12,
    'Show',
  )
  // The 379-seeder release is unplayable (HEVC) and is filtered out entirely.
  assert.deepEqual(ranked.map((release) => release.infoHash), ['seeded', 'barely'])
})

test('rankReleases prefers a healthy release over a better-quality unseeded one', () => {
  const ranked = rankReleases(
    [
      { title: 'Show - 12 (1080p x264 AAC)', seeders: 0, infoHash: 'dead' },
      { title: 'Show - 12 (720p x264 AAC)', seeders: 60, infoHash: 'healthy' },
    ],
    12,
    'Show',
  )
  // 1080p scores higher, but the 720p release has real seeders, so it is tried first.
  assert.deepEqual(ranked.map((release) => release.infoHash), ['healthy', 'dead'])
})

test('rankReleases keeps unseeded releases as a last resort', () => {
  // An indexer can report zero seeders for a swarm that is in fact alive, so these
  // are not discarded.
  const ranked = rankReleases(
    [{ title: 'Show - 12 (1080p x264 AAC)', seeders: 0, infoHash: 'only' }],
    12,
    'Show',
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['only'])
})

test('rankReleases never returns a release for a different show', () => {
  const ranked = rankReleases(
    [
      { title: 'Boruto - Naruto Next Generations Box Set 1 [1080p]', seeders: 999, infoHash: 'x' },
      { title: 'Naruto S01E01 1080p x264 AAC', seeders: 2, infoHash: 'y' },
    ],
    1,
    'Naruto',
  )
  assert.deepEqual(ranked.map((release) => release.infoHash), ['y'])
})

test('isBatch recognises every range separator release groups use', () => {
  // "01 ~ 12" is a 12-episode batch. It was read as episode 1, so a whole season was
  // ranked as a single episode and offered for episode 1 - which is worse than
  // finding nothing, because it looks like it worked.
  assert.equal(isBatch('[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]'), true)
  assert.equal(isBatch('Show 01 ~ 12 [1080p]'), true)
  assert.equal(isBatch('Show 1~12'), true)
  assert.equal(isBatch('Show 1–12'), true, 'en dash')
  assert.equal(isBatch('Show 1—12'), true, 'em dash')
  assert.equal(isBatch('Show 1-12'), true)
  // A single episode must not be caught by any of the above.
  assert.equal(isBatch('Show - 12 (1080p)'), false)
  assert.equal(isBatch('Show S01E01 [1080p]'), false)
})

test('a range release is penalised like the batch it is', () => {
  // Once "01 ~ 12" is recognised as a range, the batch penalty applies to it, so a
  // real single episode wins at equal seeder health. Before the fix the range was not
  // seen as a batch at all, so it carried no penalty.
  const titles = ['Bocchi the Rock']
  const releases = [
    { title: '[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]', infoHash: 'a'.repeat(40), seeders: 10 },
    { title: '[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv', infoHash: 'b'.repeat(40), seeders: 10 },
  ]

  const ranked = rankReleases(releases, 1, titles[0], titles)
  assert.equal(ranked[0].title, '[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv')

  // Seeder health is still the first key, by design: a batch with far more peers is
  // more likely to play than a single episode with almost none.
  const seeded = [
    { title: '[Erai-raws] Bocchi the Rock! - 01 ~ 12 [1080p][Multiple Subtitle]', infoHash: 'a'.repeat(40), seeders: 56 },
    { title: '[SubsPlease] Bocchi the Rock! - 01 (1080p) [E04F4EFB].mkv', infoHash: 'b'.repeat(40), seeders: 12 },
  ]
  assert.equal(rankReleases(seeded, 1, titles[0], titles)[0].seeders, 56)
})

// --- season packs ------------------------------------------------------------
//
// Regional and Indian series are released almost entirely as season packs
// ("Gyaarah Gyaarah S01 [1080p ...]", "Panchayat S02 Complete ..."), and a pack names
// the season instead of the episode. Before this, every one of them was found by the
// indexer and then discarded by the ranking, so a series that exists only as a pack
// reported "no torrent found" while knaben and torrents-csv held it with 71 seeders.

test('parseSeason reads a bare season token as well as SxxExx', () => {
  // A pack names only the season, which is what made a season-2 pack look like it
  // named no season at all - and therefore satisfy a request for season 1.
  assert.equal(parseSeason('Gyaarah Gyaarah S01 [1080p WEB-DL x264]'), 1)
  assert.equal(parseSeason('Panchayat S02 Complete Season 2 1080p'), 2)
  assert.equal(parseSeason('Show.Season.3.1080p'), 3)

  // The digits must not be read out of a resolution or a year.
  assert.equal(parseSeason('Show 1080p WEB-DL'), null)
  assert.equal(parseSeason('Show S2020 1080p'), null)
  assert.equal(parseSeason('Show 05 1080p'), null)
})

test('a season pack is offered only for the season it covers', () => {
  const titles = ['Panchayat']

  const seasonOne = [{ title: 'Panchayat S01 1080p WEB-DL', infoHash: 'a'.repeat(40), seeders: 30 }]
  assert.equal(rankReleases(seasonOne, 1, titles[0], titles, 1).length, 1, 'season 1 pack, season 1 request')

  const seasonTwo = [{ title: 'Panchayat S02 Complete 1080p', infoHash: 'b'.repeat(40), seeders: 30 }]
  assert.equal(rankReleases(seasonTwo, 1, titles[0], titles, 1).length, 0, 'season 2 pack must not satisfy season 1')

  // The same pack is correct for its own season.
  assert.equal(rankReleases(seasonTwo, 1, titles[0], titles, 2).length, 1)
})

test('a pack is ordered behind a release that names the episode', () => {
  // A release naming the episode is always the better answer: it is the one episode
  // rather than a whole season to reach it from.
  const titles = ['Gyaarah Gyaarah']
  const releases = [
    { title: 'Gyaarah Gyaarah S01 [1080p WEB-DL x264 AAC Hindi]', infoHash: 'a'.repeat(40), seeders: 71 },
    { title: 'Gyaarah Gyaarah S01E01 1080p WEB-DL', infoHash: 'b'.repeat(40), seeders: 2 },
  ]

  const ranked = rankReleases(releases, 1, titles[0], titles, 1)
  assert.equal(ranked.length, 2)
  assert.equal(ranked[0].infoHash, 'b'.repeat(40), 'the named episode wins despite far fewer seeders')
  assert.equal(ranked[1].infoHash, 'a'.repeat(40))
})

test('a pack is a last resort, behind a single-file fallback too', () => {
  const titles = ['Show']
  const releases = [
    { title: 'Show S01 1080p x264', infoHash: 'a'.repeat(40), seeders: 50 },
    { title: 'Show 1080p WEB-DL x264', infoHash: 'b'.repeat(40), seeders: 50 },
  ]

  const ranked = rankReleases(releases, 1, titles[0], titles, 1)
  assert.equal(ranked.length, 2)
  assert.equal(ranked[0].infoHash, 'b'.repeat(40), 'the release naming no episode is not a pack and comes first')
})

test('the anime path admits only a first-season pack', () => {
  // An anime episode id is a flat count with no season, so a later season's pack
  // cannot be mapped onto it: Shippuden S02 holds episodes 33-53 but names its files
  // 01-21, so reading file "01" out of it would serve episode 33 as episode 1.
  const titles = ['Naruto Shippuden']
  const first = [{ title: 'Naruto Shippuden S01 1080p', infoHash: 'a'.repeat(40), seeders: 20 }]
  const second = [{ title: 'Naruto Shippuden S02 1080p', infoHash: 'b'.repeat(40), seeders: 20 }]

  assert.equal(rankReleases(first, 1, titles[0], titles, null).length, 1)
  assert.equal(rankReleases(second, 1, titles[0], titles, null).length, 0)
})

test('a series search asks for the season and the title, not only the episode', () => {
  // knaben requires every word of a term to match, so an episode-bearing term finds
  // nothing for a pack-only series. Measured: "Mirzapur S01E01" returns 0 hits where
  // "Mirzapur S01" returns 15 and "Mirzapur" returns 100.
  const terms = seriesSearchTerms(['Mirzapur'], 1, 1)
  assert.ok(terms.includes('Mirzapur S01E01'), `got ${JSON.stringify(terms)}`)
  assert.ok(terms.includes('Mirzapur S01'), `got ${JSON.stringify(terms)}`)
  assert.ok(terms.includes('Mirzapur'), `got ${JSON.stringify(terms)}`)
  // The episode-specific forms still come first, so they decide the ordering whenever
  // they return anything.
  assert.ok(terms.indexOf('Mirzapur S01E01') < terms.indexOf('Mirzapur S01'))
})

test('the title term is derived without eating a real word', () => {
  assert.equal(titleOf('Show S01E01'), 'Show')
  assert.equal(titleOf('Show S01'), 'Show')
  assert.equal(titleOf('Show 001'), 'Show')
  // No episode or season on the end, so nothing is stripped.
  assert.equal(titleOf('Attack on Titan'), 'Attack on Titan')
})

test('a pack spanning several seasons is refused', () => {
  // Its file names restart at 01 for each season, so "S03E05.mkv" is indistinguishable
  // from "S01E05.mkv" by number alone, and for anime there is no season to check at all.
  const titles = ['Show']
  const spanning = [{ title: 'Show S01-S05 Complete 1080p x264', infoHash: 'a'.repeat(40), seeders: 50 }]
  assert.equal(rankReleases(spanning, 5, titles[0], titles, 3).length, 0, 'series')
  assert.equal(rankReleases(spanning, 5, titles[0], titles, null).length, 0, 'anime')
  assert.equal(rankReleases(spanning, 1, titles[0], titles, 1).length, 0)
})

test('an extras pack is not offered as an episode', () => {
  // A specials pack names no episode, so without this it lands in the pack group and is
  // offered for a real episode of that season.
  const titles = ['Show']
  const specials = [{ title: 'Show S01 Specials 1080p x264', infoHash: 'a'.repeat(40), seeders: 50 }]
  assert.equal(rankReleases(specials, 1, titles[0], titles, 1).length, 0)

  const ovas = [{ title: 'Show S01 OVA 1080p', infoHash: 'b'.repeat(40), seeders: 50 }]
  assert.equal(rankReleases(ovas, 1, titles[0], titles, 1).length, 0)
})

test('a pack with no quality tag is still offered', () => {
  // The terse regional release names this group exists for carry no quality tag at all,
  // so a pack must not be dropped for scoring low. It is the last resort, not an
  // unusable release.
  const titles = ['Show']
  const bare = [{ title: 'Show S01', infoHash: 'a'.repeat(40), seeders: 0, sizeBytes: null }]
  assert.equal(rankReleases(bare, 1, titles[0], titles, 1).length, 1)
})

test('every title variant keeps its season and title forms', () => {
  // A show is known by several names and releases use them inconsistently. Capping the
  // whole term list meant the first variant consumed the budget and the rest lost their
  // season and bare-title forms - the forms a pack-only series is found by.
  const terms = seriesSearchTerms(['Shingeki no Kyojin', 'Attack on Titan'], 1, 1)
  assert.ok(terms.includes('Shingeki no Kyojin S01'), `got ${JSON.stringify(terms)}`)
  assert.ok(terms.includes('Attack on Titan S01'), `got ${JSON.stringify(terms)}`)
  assert.ok(terms.includes('Attack on Titan'), `got ${JSON.stringify(terms)}`)

  // An anime has three variants from AniList, and the Japanese one is last.
  const anime = searchTerms(['Attack on Titan', 'Shingeki no Kyojin', '進撃の巨人'], 1)
  assert.ok(anime.some((term) => term.startsWith('進撃の巨人')), `got ${JSON.stringify(anime)}`)
})

test('a multi-season pack is refused in every form it is written', () => {
  // Release names write a season range with a hyphen, a tilde, or a space, and
  // underscores and dots have already become spaces by the time this runs. All of them
  // mean the same thing: the file names inside restart at 01 for each season, so a file
  // number alone cannot say which season it belongs to.
  const titles = ['Show']
  for (const title of [
    'Show S01-S05 Complete 1080p x264',
    'Show.S01.S05.Complete.1080p',
    'Show S01 S05 1080p',
    'Show S01~S05 Complete',
  ]) {
    const releases = [{ title, infoHash: 'a'.repeat(40), seeders: 50 }]
    assert.equal(rankReleases(releases, 5, titles[0], titles, null).length, 0, `${title} (anime)`)
    assert.equal(rankReleases(releases, 5, titles[0], titles, 3).length, 0, `${title} (series)`)
  }
})

test('a special numbered 1 is not episode 1', () => {
  // "Show S01 Special 1 1080p" parses to episode 1 and would otherwise land in the
  // named group and outrank the real episode on seeders or resolution.
  const titles = ['Show']
  const special = [{ title: 'Show S01 Special 1 1080p x264', infoHash: 'a'.repeat(40), seeders: 99 }]
  assert.equal(rankReleases(special, 1, titles[0], titles, 1).length, 0)
})

// A title with an apostrophe is written three ways and the variants do not match each
// other: the catalogue spelling ("India's Got Latent"), the scene spelling that drops
// the apostrophe ("Indias Got Latent"), and the spaced form that normalizeTitle
// produces ("India s Got Latent"). Only the last was ever compared, so real releases
// were rejected by the relevance gate and the search terms matched nothing anywhere.

test('an apostrophe title matches every spelling a release uses', () => {
  const titles = ["India's Got Latent"]
  for (const release of [
    'INDIA S GOT LATENT S 2 EP 5 Ft. Orry Archana Puran Singh',
    'INDIA S GOT LATENT S 2 EP 6 Ft. Rakhi Sawant',
    'Indias Got Latent S02E01 1080p',
    'Indias.Got.Latent.S02E01.1080p.WEB-DL',
    "India's Got Latent S02E01 1080p",
  ]) {
    assert.equal(matchesAnyTitle(release, titles), true, release)
  }
})

test('the apostrophe variants do not make the gate permissive', () => {
  const titles = ["India's Got Latent"]
  // A different show that shares every word except the first must still be rejected.
  assert.equal(matchesAnyTitle('Britain s Got Latent Problems', titles), false)
  assert.equal(matchesAnyTitle("Britain's Got Latent", titles), false)
  assert.equal(matchesAnyTitle('Got Latent', titles), false)
})

test('an apostrophe title is searched in every spelling', () => {
  const terms = seriesSearchTerms(["India's Got Latent"], 2, 1)
  // The apostrophe is dropped in the spelling real releases use, so a term without it
  // must exist; without this every indexer returned zero rows for the show.
  assert.ok(terms.includes('Indias Got Latent S02E01'), terms.join(' | '))
  assert.ok(terms.includes('Indias Got Latent'), terms.join(' | '))
  // The spaced form is kept too: archive.org names its items that way.
  assert.ok(terms.includes('India s Got Latent'), terms.join(' | '))
})

test('a title without an apostrophe is unaffected', () => {
  assert.deepEqual(titleVariants('Panchayat'), ['panchayat'])
  assert.deepEqual(titleVariants('Bocchi the Rock!'), ['bocchi the rock'])
})

// archive.org writes the season out longhand and with gaps: "INDIA S GOT LATENT S 2 EP 6",
// "Panchayat S 03 E 1 8", "Mirzapur. S 01. E 01". A release whose season cannot be read
// passes the season gate vacuously, so a season-2 file would satisfy a season-1 request.

test('parseSeason reads the spaced season form archive.org uses', () => {
  assert.equal(parseSeason('INDIA S GOT LATENT S 2 EP 5 Ft. Orry'), 2)
  assert.equal(parseSeason('Panchayat S 03 E 1 8'), 3)
  assert.equal(parseSeason('Mirzapur. S 01. E 01.720p. HEVC'), 1)
  assert.equal(parseSeason('Gyaarah. Gyaarah. S 01 E 01. Episode. 1.1080p'), 1)
})

test('the spaced season form still refuses a resolution and a year', () => {
  // The widening must not turn "1080p" or "S2020" into a season.
  assert.equal(parseSeason('Show 1080p WEB-DL'), null)
  assert.equal(parseSeason('Show S2020 1080p'), null)
  assert.equal(parseSeason('Show S1 1080p'), 1, 'a bare S1 is still a season')
})

test('a spaced-season file is rejected for the wrong season', () => {
  const titles = ['India s Got Latent']
  const pack = [{ title: 'INDIA S GOT LATENT S 2 EP 5 Ft. Orry', infoHash: 'a'.repeat(40), seeders: 3 }]
  // The release names episode 5 of season 2, so it is not episode 1 of season 1.
  assert.equal(matchesEpisode(pack[0].title, 1, 1), false)
  assert.equal(matchesEpisode(pack[0].title, 5, 2), true)
  assert.equal(rankReleases(pack, 1, titles[0], titles, 1).length, 0)
  assert.equal(rankReleases(pack, 5, titles[0], titles, 2).length, 1)
})

test('pickVideoFile prefers a real episode over a bonus of the same number', async () => {
  const { pickVideoFile } = await import('../graphql/stream/torrent.js')

  // The real Season 1 pack of "India's Got Latent". "IGL_Bonus_EP_1" and "IGL_EP_01"
  // both parse as episode 1, and the bonus is listed first - so the bonus was served in
  // place of the episode. The pack holds every real episode at 47 seeders.
  const pack = [
    'IGL/IGL_Bonus_EP_1_ft. Arpit Bala.mp4',
    'IGL/IGL_EP_01_ft_RaftaarMusic.mp4',
    'IGL/IGL_EP_02_ft_GamerFleet_JokeSingh.mp4',
    'IGL/IGL_EP_05_ft_KunalKamra_AtulKhatri.mp4',
    'IGL/IGL_BONUS_EP_Discarded_1_ft_Aakash_Gupta.mp4',
  ].map((name) => ({ name, length: 500 * 1024 ** 2 }))

  assert.match(pickVideoFile(pack, 1, 0, 1).name, /IGL_EP_01/, 'the episode must win over the bonus')
  assert.match(pickVideoFile(pack, 2, 0, 1).name, /IGL_EP_02/)
  assert.match(pickVideoFile(pack, 5, 0, 1).name, /IGL_EP_05/)

  // A pack holding only a bonus still serves it, because there is nothing else it
  // could be - refusing outright would turn a playable file into no source at all.
  const bonusOnly = [{ name: 'IGL/IGL_Bonus_EP_1_ft. Arpit Bala.mp4', length: 500 * 1024 ** 2 }]
  assert.match(pickVideoFile(bonusOnly, 1, 0, 1).name, /Bonus_EP_1/)
})
