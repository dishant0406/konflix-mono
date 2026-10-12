import dynamic from 'next/dynamic'
import Head from 'next/head'
import { useRouter } from 'next/router'
import { AiFillYoutube } from 'react-icons/ai'

import { AnimeHero } from '@/components'
import { GET_SINGLE_ANIME } from 'utils/graphql/Queries'
import { query } from 'utils/graphql/client'
import { poppinsBold } from '@/utils/Fonts/fonts'

const ReactPlayer = dynamic(() => import('react-player'), { ssr: false })

const Info = ({ anime }) => {
  const router = useRouter()
  const details = anime?.animeDetails

  if (!details) {
    return (
      <div className="bg-[#05161e] min-h-[100vh] flex items-center justify-center">
        <p className="text-white">This anime could not be found.</p>
      </div>
    )
  }

  return (
    <>
      <Head>
        <title>{details.name} | Konflix</title>
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <meta charSet="utf-8" />
        <link rel="icon" href="/Images/favicon.ico" />
        <meta name="description" content={details.synopsis} />
        <meta property="og:title" content={details.name} />
        <meta property="og:description" content={details.synopsis} />
        <meta property="og:type" content="website" />
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:title" content={details.name} />
        <meta name="twitter:description" content={details.synopsis} />
      </Head>

      <div className="bg-[#05161e] min-h-[100vh] pb-[50px]">
        <AnimeHero anime={anime} animeId={router.query.id} />

        <div className="w-[100vw] mt-[10rem] md:mt-[1rem] flex justify-center">
          <div className="mt-[6rem] w-[90vw]">
            <p
              style={{ textShadow: '0px 0px 10px rgba(0,0,0,1)' }}
              className={`${poppinsBold.className} opacity-25 text-[18px] md:text-[20px] tracking-tight font-bold text-white`}
            >
              DESCRIPTION
            </p>
            <p
              style={{ textShadow: '0px 0px 10px rgba(0,0,0,1)' }}
              className={`${poppinsBold.className} text-[14px] md:text-[16px] tracking-tight font-bold mb-[1rem] text-white whitespace-pre-line`}
            >
              {details.synopsis}
            </p>
          </div>
        </div>

        {/* The trailer is optional: many titles have none. */}
        {details.trailer && (
          <div className="w-[100vw] flex justify-center">
            <div className="md:mt-[1rem] w-[90vw]">
              <p
                style={{ textShadow: '0px 0px 10px rgba(0,0,0,1)' }}
                className={`${poppinsBold.className} opacity-25 text-[18px] md:text-[20px] tracking-tight font-bold text-white`}
              >
                TRAILER
              </p>
              <div className="rounded-[10px] overflow-hidden">
                <ReactPlayer
                  playIcon={
                    <div className="bg-white scale-150 hover:scale-125 transition-all duration-300">
                      <AiFillYoutube className="text-[#38a835] scale-[3]" />
                    </div>
                  }
                  style={{ boxShadow: '0px 0px 10px rgba(0,0,0,1)' }}
                  width="90vw"
                  light
                  url={details.trailer}
                />
              </div>
            </div>
          </div>
        )}
      </div>
    </>
  )
}

export async function getServerSideProps(context) {
  try {
    const { data } = await query({
      query: GET_SINGLE_ANIME,
      variables: { animeDetailsId: context.params.id },
    })

    if (!data.animeDetails) return { notFound: true }

    return { props: { anime: data } }
  } catch (error) {
    console.error(`[info] could not load anime: ${error.message}`)
    return { notFound: true }
  }
}

export default Info
