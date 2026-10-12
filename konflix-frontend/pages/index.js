import { useEffect, useState } from 'react'
import { ClipLoader } from 'react-spinners'
import { MdOutlineNavigateNext, MdOutlineNavigateBefore } from 'react-icons/md'

import { Hero, TrendingWeek, RecentAnimes } from 'components'
import { GET_TOP_ANIME, GET_RECENT_ANIME } from 'utils/graphql/Queries'
import { query } from 'utils/graphql/client'

const Home = ({ topAnime, recentEpisodes }) => {
  const [selectedAnime, setSelectedAnime] = useState(topAnime[0])
  const [recentEpi, setRecentEpisodes] = useState(recentEpisodes)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(null)

  // Props are refetched on every visit, but the component instance is reused
  // during client-side navigation, so the selection is resynced when they change.
  useEffect(() => {
    setSelectedAnime(topAnime[0])
    setRecentEpisodes(recentEpisodes)
    setPage(1)
  }, [topAnime, recentEpisodes])

  // The API returns an empty list when a page is past the end, which is the only
  // signal available for disabling the next button.
  const hasNextPage = recentEpi.length > 0
  const isEmpty = topAnime.length === 0 && recentEpi.length === 0
  const goToPage = async (nextPage) => {
    setLoading(true)
    setError(null)
    try {
      const { data } = await query({ query: GET_RECENT_ANIME, variables: { page: nextPage } })
      setRecentEpisodes(data.recentEpisodes)
      setPage(nextPage)
    } catch (requestError) {
      setError(requestError.message)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="bg-black index relative">
      <div className="fixed top-[40%] right-[48%]">
        <ClipLoader loading={loading} color="#fff" size={50} />
      </div>
      <div className={`min-h-[100vh] ${loading ? 'opacity-50' : ''} w-[100vw] pb-[125px] bg-[#05161e]`}>
        {/* When the API is unreachable every section is empty, so say so instead of
            rendering a page of blank rows. */}
        {isEmpty ? (
          <div className="h-[60vh] flex flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-white font-bold text-lg">Could not reach the Konflix API</p>
            <p className="text-white/60 text-sm max-w-[40rem]">
              No anime could be loaded. Check that the API is running and that
              NEXT_PUBLIC_GRAPHQL_URI points at it, then reload.
            </p>
          </div>
        ) : (
          <>
            <Hero
              selectedAnime={selectedAnime}
              setSelectedAnime={setSelectedAnime}
              topAnimes={topAnime}
            />
            <TrendingWeek topAnimes={topAnime} />
            <RecentAnimes recentAnimes={recentEpi} />
          </>
        )}
        {error && (
          <p className="w-[90vw] mx-auto mt-[1rem] text-center text-sm text-white/60">
            Could not load the next page: {error}
          </p>
        )}

        <div className="w-[100vw] gap-[1rem] flex justify-center">
          <button
            disabled={page === 1 || loading}
            style={{
              boxShadow: '0 8px 32px 0 rgba( 31, 38, 100, 0.69 )',
              backdropFilter: 'blur(10px)',
              backgroundColor: 'rgba( 0,0,0,0.05)',
            }}
            className={`text-white ${page === 1 ? 'opacity-[0.3]' : ''} py-2 mt-[2rem] px-2 rounded-lg`}
            onClick={() => goToPage(page - 1)}
          >
            <MdOutlineNavigateBefore className="text-white" />
          </button>
          <button
            disabled={!hasNextPage || loading}
            style={{
              boxShadow: '0 8px 32px 0 rgba( 31, 38, 100, 0.69 )',
              backdropFilter: 'blur(10px)',
              backgroundColor: 'rgba( 0,0,0,0.05)',
            }}
            className={`text-white ${!hasNextPage ? 'opacity-[0.3]' : ''} py-2 mt-[2rem] px-2 rounded-lg`}
            onClick={() => goToPage(page + 1)}
          >
            <MdOutlineNavigateNext className="text-white" />
          </button>
        </div>
      </div>
    </div>
  )
}

export async function getServerSideProps() {
  try {
    const [top, recent] = await Promise.all([
      query({ query: GET_TOP_ANIME, variables: { page: 1 } }),
      query({ query: GET_RECENT_ANIME, variables: { page: 1 } }),
    ])

    return {
      props: {
        topAnime: top.data.topAnime || [],
        recentEpisodes: recent.data.recentEpisodes || [],
      },
    }
  } catch (error) {
    // The page must render even when the API is unreachable, so a failure here
    // degrades to empty sections instead of an error page.
    console.error(`[home] could not load anime: ${error.message}`)
    return { props: { topAnime: [], recentEpisodes: [] } }
  }
}

export default Home
