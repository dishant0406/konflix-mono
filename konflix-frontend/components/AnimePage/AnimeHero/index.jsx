import { useState } from 'react'
import { useRouter } from 'next/router'
import { FaPlay } from 'react-icons/fa'
import { poppinsBold, poppinsMedium } from 'utils/Fonts/fonts'
import { Search } from 'components'
import { youtubeThumbnail } from 'utils/youtube'

const InfoCard = ({ type, title }) => (
  <div
    style={{
      boxShadow: '0 8px 32px 0 rgba( 31, 38, 135, 0.37 )',
      backdropFilter: 'blur(10px)',
      backgroundColor: 'rgba(0,0,0,0.01)',
    }}
    className="w-[120px] md:w-[200px] relative flex items-center justify-center h-[80px] md:h-[120px] rounded-[10px] md:rounded-[20px]"
  >
    <p
      className={`${poppinsMedium.className} text-white absolute left-[-10px] md:left-[-15px] -rotate-90 opacity-50 text-[8px] md:text-[12px]`}
    >
      {type}
    </p>
    <p className={`${poppinsBold.className} text-white text-[14px] md:text-[24px]`}>{title || 'N/A'}</p>
  </div>
)

const AnimeHero = ({ anime, animeId }) => {
  const router = useRouter()
  const details = anime?.animeDetails
  const background = youtubeThumbnail(details?.trailer)

  if (!details) return null

  const firstEpisode = details.episode_id?.[0]?.episodeId

  return (
    <div
      className="w-[100vw] transition-all duration-300 items-center h-[50vh] flex flex-col justify-between"
      style={{
        backgroundImage: background
          ? `url(${background})`
          : details.imageUrl
            ? `url(${details.imageUrl})`
            : undefined,
        backgroundColor: background || details.imageUrl ? undefined : '#05161e',
        backgroundSize: 'cover',
        backgroundPosition: 'center',
        backgroundRepeat: 'no-repeat',
        boxShadow: '0px -40px 45px -1px rgba(5,22,30,1) inset',
      }}
    >
      <Search />

      <div className="w-[90vw]">
        <p
          style={{ textShadow: '0px 0px 10px rgba(0,0,0,1)' }}
          className={`${poppinsBold.className} transition-all duration-300 text-[38px] md:text-[56px] tracking-tight font-bold text-white`}
        >
          {details.name?.length > 35 ? `${details.name.slice(0, 35).toUpperCase()}...` : details.name?.toUpperCase()}
        </p>

        <div className="flex flex-wrap mb-[2rem] gap-[10px]">
          {details.genres?.map((genre) => (
            <div key={genre} className="bg-[#686868] text-white border border-white rounded-md px-[5px] py-[2px]">
              <p className={`${poppinsMedium.className} text-[14px]`}>{genre}</p>
            </div>
          ))}
        </div>

        <div className="w-[90vw] flex flex-wrap justify-center gap-[40px] mb-[-3rem]">
          {/* Play is disabled rather than broken when there are no episodes. */}
          <div className="md:w-[200px] md:inline-block md:mt-[0] mt-[2rem] w-[300px] flex justify-center">
            <button
              type="button"
              disabled={!firstEpisode}
              onClick={() => router.push(`/watch/${animeId}/${firstEpisode}`)}
              className={`w-[200px] flex items-center justify-center transition-all duration-300 h-[120px] rounded-[20px] ${
                firstEpisode
                  ? 'bg-[#39a936] hover:scale-[1.15] border-white border-[2px] scale-110 cursor-pointer'
                  : 'bg-white/10 cursor-not-allowed'
              }`}
            >
              <FaPlay className="text-white scale-[2]" />
            </button>
          </div>
          <InfoCard title={details.status} type="STATUS" />
          <InfoCard title={details.totalEpisodes} type="EPISODES" />
          <InfoCard title={details.released} type="RELEASE YEAR" />
          <InfoCard title={details.type} type="FORMAT" />
        </div>
      </div>
    </div>
  )
}

export default AnimeHero
