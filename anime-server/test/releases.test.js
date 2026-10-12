import test from 'node:test'
import assert from 'node:assert/strict'

import { parseEpisode, isBatch, isExtra, isLikelyMovie, matchesAnyTitle, parseResolution, parseCodec, parseAudio, scoreRelease, rankReleases, searchTerms, isVideoFile, isRelevant, matchesEpisode } from '../graphql/stream/releases.js'

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
