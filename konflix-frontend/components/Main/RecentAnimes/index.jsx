import { useRouter } from 'next/router'
import { AiFillYoutube } from 'react-icons/ai'
import { poppinsMedium } from 'utils/Fonts/fonts'

const RecentAnimes = ({ recentAnimes = [], title }) => {
  const router = useRouter()

  return (
    <div className="w-[100vw] flex flex-col justify-center items-center">
      <div className="w-[90vw]">
        <p className={`${poppinsMedium.className} text-white text-[20px] font-bold`}>
          {title || '🔥 RECENT EPISODES'}
        </p>
        <div className="mt-[1rem] flex gap-[1rem] justify-center md:justify-start flex-wrap w-[90vw]">
          {recentAnimes.map((episode) => (
            <div key={episode.episodeId} className="relative">
              <div
                onClick={() => router.push(`/watch/${episode.animeId}/${episode.episodeId}`)}
                style={{
                  backgroundImage: `url(${episode.imgUrl})`,
                  backgroundSize: 'cover',
                  backgroundPosition: 'center',
                  backgroundRepeat: 'no-repeat',
                }}
                className="w-[130px] h-[200px] rounded-[20px] cursor-pointer flex items-center justify-center"
              >
                <AiFillYoutube className="text-[#38a835] scale-[2] opacity-0 hover:opacity-100" />
              </div>
              <p className={`${poppinsMedium.className} text-[13px] text-white mt-[6px] w-[130px] truncate`}>
                {episode.name}
              </p>
              <span className="absolute top-[8px] right-[8px] bg-red-600 text-white text-[11px] font-bold rounded-md px-[6px] py-[2px]">
                Ep {episode.episodeNum}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default RecentAnimes
