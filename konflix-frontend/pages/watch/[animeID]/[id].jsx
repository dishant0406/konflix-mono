import { useEffect, useMemo, useRef, useState } from 'react'
import { useRouter } from 'next/router'
import Head from 'next/head'
import dynamic from 'next/dynamic'
import { ClipLoader } from 'react-spinners'
import { MdOutlineNavigateNext, MdOutlineNavigateBefore } from 'react-icons/md'

import { AnimeCard, Search } from 'components'
import { GET_STREAM_LINK, GET_SINGLE_ANIME, GET_RECOMMEDED_ANIME } from '@/utils/graphql/Queries'
import { query } from '@/utils/graphql/client'
import { poppinsMedium } from '@/utils/Fonts/fonts'
// plyr touches `document` when it is imported, so it can only be loaded in the
// browser.
const Plyr = dynamic(() => import('plyr-react'), { ssr: false })

function titleFromSlug(slug) {
  if (!slug) return ''
  return slug
    .split('-')
    .filter(Boolean)
    .map((word) => word[0].toUpperCase() + word.slice(1))
    .join(' ')
}

const PLYR_OPTIONS = {
  controls: [
    'play-large',
    'restart',
    'rewind',
    'play',
    'fast-forward',
    'progress',
    'current-time',
    'duration',
    'mute',
    'volume',
    'settings',
    'pip',
    'fullscreen',
  ],
  settings: ['speed'],
  speed: { selected: 1, options: [0.5, 0.75, 1, 1.25, 1.5, 2] },
  keyboard: { focused: true, global: true },
  tooltips: { controls: true, seek: true },
  fullscreen: { enabled: true, fallback: true, iosNative: true },
  autoplay: false,
  playsinline: true,
  invertTime: false,
  clickToPlay: true,
  hideControls: false,
  resetOnEnd: false,
  iconUrl: 'https://cdn.plyr.io/3.6.8/plyr.svg',
}

// One episode in the grid below the player.
const EpisodeButton = ({ episode, isCurrent, animeImage, onSelect }) => {
  const [isHovered, setIsHovered] = useState(false)
  const highlighted = isCurrent || isHovered

  return (
    <button
      type="button"
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onClick={() => onSelect(episode.episodeId)}
      style={{
        backgroundImage: `url(${animeImage})`,
        backgroundSize: 'cover',
        backgroundPosition: 'center',
        transform: highlighted ? 'scale(1.1)' : 'scale(1)',
        opacity: isCurrent ? 1 : 0.5,
      }}
      className="relative md:h-[3rem] h-[2rem] w-[3rem] md:w-[4rem] cursor-pointer transition-all duration-300 rounded-md bg-black"
    >
      <span
        className={`absolute bottom-[0.2rem] left-[0.8rem] md:left-[1rem] text-white text-[20px] md:text-[30px] transition-all duration-300 font-bold ${
          highlighted ? 'opacity-100' : 'opacity-0'
        }`}
      >
        {episode.episodeNumber}
      </span>
    </button>
  )
}

const Watch = () => {
  const router = useRouter()
  const { animeID, id: episodeId } = router.query

  const [stream, setStream] = useState(null)
  const [anime, setAnime] = useState({})
  const [recommended, setRecommended] = useState([])
  const [loadingStream, setLoadingStream] = useState(true)
  const [streamError, setStreamError] = useState(null)
  // Set when the browser itself refuses to play the source it was given, which is
  // different from the API reporting that it has none.
  const [playbackError, setPlaybackError] = useState(null)
  const playerRef = useRef(null)

  // Metadata and the stream are fetched independently on purpose. Metadata takes a
  // few hundred milliseconds; resolving a stream searches torrent indexers, opens a
  // torrent and probes the file, which can take a minute. Awaiting both together
  // would leave the page blank for that whole time, so the title and episode list
  // render as soon as metadata arrives.
  useEffect(() => {
    if (!animeID || !episodeId) return

    let cancelled = false
    setLoadingStream(true)
    setStreamError(null)
    setPlaybackError(null)

    query({ query: GET_SINGLE_ANIME, variables: { animeDetailsId: animeID } })
      .then(({ data }) => {
        if (cancelled) return
        const details = data.animeDetails || {}
        setAnime(details)

        const genre = details.genres?.[0]
        if (!genre) return
        query({ query: GET_RECOMMEDED_ANIME, variables: { genre } })
          .then(({ data: recommendedData }) => {
            if (!cancelled) setRecommended(recommendedData.recommandedAnime || [])
          })
          .catch(() => {
            // Recommendations are optional; a failure must not break the player.
          })
      })
      .catch(() => {
        // The player below reports the failure; the page still renders.
      })

    query({ query: GET_STREAM_LINK, variables: { streamLinkDetailsId: episodeId } })
      .then(({ data }) => {
        if (cancelled) return
        setStream(data.streamLinkDetails)
        setLoadingStream(false)
      })
      .catch((requestError) => {
        if (cancelled) return
        setStreamError(requestError.message)
        setLoadingStream(false)
      })

    return () => {
      cancelled = true
    }
  }, [animeID, episodeId])

  // plyr-react rebuilds the player whenever the `source` prop changes identity, so
  // the object must be memoised. Passing a fresh literal on every render would
  // destroy and recreate the player continuously, restarting playback.
  const playerSource = useMemo(
    () =>
      stream?.url
        ? {
            type: 'video',
            title: stream.animeNameWithEP || 'Episode',
            // No `type` on the source. The API remuxes MKV into a fragmented MP4 but
            // relays MP4 as-is, and the same extension can carry different codecs, so
            // any declared MIME type here would be a guess. The browser sniffs the
            // container either way, and a wrong type can only mislead it.
            sources: [{ src: stream.url }],
          }
        : null,
    [stream?.url, stream?.animeNameWithEP],
  )

  // `url` can be an absolute CDN link (debrid) or a path on this API (torrent
  // relay), so it is resolved against the API origin before being shown. The whole
  // page is client rendered, but this still guards the pre-render pass.
  const sourceHost = useMemo(() => {
    if (!stream?.url || typeof window === 'undefined') return null
    try {
      return new URL(stream.url, process.env.NEXT_PUBLIC_GRAPHQL_URI || window.location.origin).host
    } catch {
      return null
    }
  }, [stream?.url])

  const episodes = anime?.episode_id || []
  const currentIndex = episodes.findIndex((episode) => episode.episodeId === episodeId)
  const previous = currentIndex > 0 ? episodes[currentIndex - 1] : null
  const next = currentIndex >= 0 && currentIndex < episodes.length - 1 ? episodes[currentIndex + 1] : null

  const goToEpisode = (id) => {
    if (id) router.push(`/watch/${animeID}/${id}`)
  }

  return (
    <div className="relative">
      <Head>
        <title>{titleFromSlug(episodeId)} | Konflix</title>
        <meta
          name="description"
          content="Watch anime online in high quality. Streaming is powered by open torrent indexers."
        />
        <link rel="icon" href="/Images/favicon.ico" />
      </Head>

      <div className="w-[100vw] pb-[125px] flex flex-col items-center">
        <Search />

        <div className="flex md:gap-[1rem] gap-[0.5rem] md:flex-row flex-col items-center md:w-[70vw]">
          <h1
            onClick={() => router.push(`/info/${animeID}`)}
            className="md:text-2xl cursor-pointer text-lg font-bold text-white md:text-left mt-[10px] text-center md:mt-[1rem]"
          >
            {anime?.name || titleFromSlug(animeID)}
          </h1>
          <div className="flex gap-[0.5rem]">
            <button
              type="button"
              disabled={!previous}
              onClick={() => goToEpisode(previous?.episodeId)}
              className="bg-black disabled:opacity-30 cursor-pointer flex items-center gap-1 rounded-md h-[2rem] px-[10px] md:mt-[1rem]"
            >
              <MdOutlineNavigateBefore className="text-white" />
              <span className="text-sm font-bold text-white whitespace-nowrap">Previous</span>
            </button>
            <button
              type="button"
              disabled={!next}
              onClick={() => goToEpisode(next?.episodeId)}
              className="bg-black disabled:opacity-30 cursor-pointer flex items-center gap-1 rounded-md h-[2rem] px-[10px] md:mt-[1rem]"
            >
              <span className="text-sm font-bold text-white whitespace-nowrap">Next</span>
              <MdOutlineNavigateNext className="text-white" />
            </button>
          </div>
        </div>

        <div style={{ width: '70vw' }} className="mt-[0.5rem]">
          {loadingStream && (
            <div className="h-[40vh] flex flex-col items-center justify-center gap-3 bg-black rounded-lg px-6">
              <ClipLoader color="#fff" size={45} />
              <p className="text-white/60 text-sm text-center">
                Searching torrent indexers for a playable source. This can take up to a minute.
              </p>
            </div>
          )}

          {!loadingStream && streamError && (
            <div className="h-[40vh] flex flex-col items-center justify-center gap-2 bg-black rounded-lg px-6 text-center">
              <p className="text-white font-bold">Could not load this episode</p>
              <p className="text-white/60 text-sm">{streamError}</p>
            </div>
          )}

          {!loadingStream && !streamError && stream?.available && (
            <>
              <div className="overflow-hidden rounded-lg">
                <Plyr
                  id="plyr"
                  ref={playerRef}
                  options={PLYR_OPTIONS}
                  source={playerSource}
                  // The source is resolved server-side and points at this API, which
                  // either relays the file or remuxes it. A browser refusing to play it
                  // is reported instead of leaving a blank player, which is otherwise
                  // indistinguishable from a stream that is merely slow to buffer.
                  //
                  // The event is inspected rather than assumed: the common real failure
                  // is a codec the browser lacks, and reporting "could not play" with no
                  // detail is what makes that look like a broken site.
                  onError={(event) => {
                    const mediaError = event?.target?.error || event?.currentTarget?.error
                    setPlaybackError(
                      mediaError?.code === 4
                        ? 'Your browser cannot play this file. The release uses a codec it does not support.'
                        : 'Playback failed. The stream may have expired - reload the episode to try again.',
                    )
                  }}
                />
              </div>
              {playbackError && (
                <p className="text-amber-300/80 text-sm mt-2">{playbackError}</p>
              )}
              {stream.release && (
                <p className={`${poppinsMedium.className} text-white/50 text-[12px] mt-2 break-all`}>
                  Source: {stream.release}
                  {sourceHost ? ` · via ${sourceHost}` : ''}
                  {stream.streamType === 'remux' ? ' · remuxed for browser playback' : ''}
                </p>
              )}
            </>
          )}

          {!loadingStream && !streamError && !stream?.available && (
            <div className="h-[40vh] flex flex-col items-center justify-center gap-3 bg-black rounded-lg px-6 text-center">
              <p className="text-white font-bold text-lg">No playable source found</p>
              <p className="text-white/60 text-sm max-w-[40rem]">
                {stream?.reason || 'The API returned no stream for this episode.'}
              </p>
              <p className="text-white/40 text-xs max-w-[40rem]">
                Playback depends on public torrent indexers, which do not have every episode.
              </p>
              {next && (
                <button
                  type="button"
                  onClick={() => goToEpisode(next.episodeId)}
                  className="mt-2 bg-white text-black font-bold rounded-md px-4 py-2"
                >
                  Try episode {next.episodeNumber}
                </button>
              )}
            </div>
          )}
        </div>

        <div className="mt-[1rem] flex flex-wrap gap-[1rem] md:justify-start justify-center md:w-[90vw] w-[95vw]">
          {episodes.map((episode) => (
            <EpisodeButton
              key={episode.episodeId}
              episode={episode}
              isCurrent={episode.episodeId === episodeId}
              animeImage={anime.imageUrl}
              onSelect={goToEpisode}
            />
          ))}
        </div>

        {recommended.length > 0 && (
          <div className="w-[90vw]">
            <h2 className="text-white text-xl font-bold mt-[2rem]">Recommended Anime</h2>
            <div className="mt-[1rem] flex flex-wrap gap-[2rem] md:justify-start justify-center">
              {recommended.map((anime) => (
                <AnimeCard
                  key={anime.animeId}
                  id={anime.animeId}
                  title={anime.animeTitle}
                  image={anime.animeImg}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default Watch
