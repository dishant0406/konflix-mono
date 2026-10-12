import { useEffect, useState } from 'react'
import { useRouter } from 'next/router'
import { RiSearch2Line as CgSearch } from 'react-icons/ri'
import { poppinsBold, poppinsMedium } from 'utils/Fonts/fonts'
import { GET_SEARCH_ANIME } from '@/utils/graphql/Queries'
import { query } from '@/utils/graphql/client'

const Search = () => {
  const router = useRouter()
  const [term, setTerm] = useState('')
  const [results, setResults] = useState([])

  useEffect(() => {
    if (!term.trim()) {
      setResults([])
      return
    }

    // Debounced so a search is only sent once typing pauses. The cancelled flag
    // is declared outside the timer so that cleanup can invalidate a request that
    // is already in flight; otherwise a slow earlier search can overwrite the
    // results of a newer one.
    let cancelled = false
    const timer = setTimeout(() => {
      query({ query: GET_SEARCH_ANIME, variables: { animeSearchId: term } })
        .then(({ data }) => {
          if (!cancelled) setResults(data.animeSearch || [])
        })
        .catch(() => {
          if (!cancelled) setResults([])
        })
    }, 400)

    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [term])

  return (
    <div className="w-[90vw] flex justify-between mt-[0.5rem] md:mt-[1rem]">
      <p
        style={{ textShadow: '0px 0px 10px rgba(0,0,0,1)' }}
        onClick={() => router.push('/')}
        className={`${poppinsBold.className} text-white cursor-pointer text-[30px] font-bold`}
      >
        KonFlix
      </p>

      <div className="relative">
        <input
          style={{
            boxShadow: '0 8px 32px 0 rgba( 31, 38, 135, 0.37 )',
            backdropFilter: 'blur(10px)',
            backgroundColor: 'rgba( 0,0,0,0.45)',
          }}
          value={term}
          onChange={(event) => setTerm(event.target.value)}
          type="text"
          autoComplete="off"
          placeholder="Search for an anime"
          className={`${poppinsMedium.className} pr-[2.5rem] max-w-[300px] tracking-wider text-white !outline-none px-[1rem] h-[2.5rem] border-none rounded-2xl`}
        />
        <div className="absolute right-[20px] top-[12px]">
          <CgSearch className="text-white scale-[1.30]" />
        </div>

        {results.length > 0 && (
          <div
            style={{
              boxShadow: '0 8px 32px 0 rgba( 31, 38, 135, 0.37 )',
              backdropFilter: 'blur(10px)',
              backgroundColor: 'rgba( 0,0,0,0.45)',
            }}
            className="w-[220px] absolute top-[45px] overflow-hidden rounded-2xl left-0 z-50"
          >
            {results.slice(0, 5).map((result) => (
              <div
                key={result.id}
                onClick={() => {
                  router.push(`/info/${result.id}`)
                  setTerm('')
                }}
                className="flex cursor-pointer transition-all duration-300 hover:scale-105 justify-start gap-[5px] py-[10px]"
              >
                <div
                  style={{
                    backgroundImage: `url(${result.imgUrl})`,
                    backgroundSize: 'cover',
                    backgroundPosition: 'center',
                    backgroundRepeat: 'no-repeat',
                  }}
                  className="h-[40px] ml-[20px] w-[40px] rounded-2xl"
                />
                <p className={`${poppinsMedium.className} text-[14px] text-white mt-[8px]`}>
                  {result.title?.length > 20 ? `${result.title.slice(0, 20)}...` : result.title}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export default Search
