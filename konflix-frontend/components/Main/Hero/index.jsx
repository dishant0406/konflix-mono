import { useRouter } from 'next/router'
import { AiFillYoutube } from 'react-icons/ai'
import { poppinsBold } from 'utils/Fonts/fonts'
import { Search } from 'components'
import { youtubeThumbnail } from 'utils/youtube'

const Hero = ({ selectedAnime, topAnimes = [], setSelectedAnime }) => {
  const router = useRouter()
  const background = youtubeThumbnail(selectedAnime?.trailer)

  return (
    <div
      className="w-[100vw] transition-all duration-300 items-center h-[50vh] flex flex-col justify-between"
      style={{
        backgroundImage: background ? `url(${background})` : undefined,
        backgroundColor: background ? undefined : '#05161e',
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
          className={`${poppinsBold.className} transition-all duration-300 text-[38px] md:text-[56px] tracking-tight font-bold mb-[1rem] text-white`}
        >
          {selectedAnime?.title?.toUpperCase()}
        </p>

        {/* Clicking a tile only previews it, so the title needs its own link to the
            details page; the play icon on a tile is the primary action instead. */}
        {selectedAnime?.id && (
          <button
            type="button"
            onClick={() => router.push(`/info/${selectedAnime.id}`)}
            className={`${poppinsBold.className} mb-[1rem] bg-white text-black text-[14px] font-bold rounded-md px-[1rem] py-[0.4rem] transition-all duration-300 hover:scale-105`}
          >
            View details
          </button>
        )}
        <div className="mb-[-3rem] gap-[40px] hidden md:flex">
          {topAnimes.slice(0, 6).map((anime) => {
            const isSelected = selectedAnime?.id === anime.id
            return (
              <div
                key={anime.id}
                onClick={() => setSelectedAnime(anime)}
                style={{
                  backgroundImage: `url(${anime.imgUrl})`,
                  backgroundSize: 'cover',
                  backgroundPosition: 'center',
                  backgroundRepeat: 'no-repeat',
                }}
                className={`w-[200px] flex items-center justify-center cursor-pointer transition-all duration-300 ${
                  isSelected ? 'border-white border-[2px] scale-110' : 'opacity-50'
                } h-[120px] rounded-[20px]`}
              >
                {isSelected && (
                  <button
                    type="button"
                    aria-label={`View ${anime.title}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      router.push(`/info/${anime.id}`)
                    }}
                    className="bg-white scale-110 hover:scale-125 transition-all duration-300"
                  >
                    <AiFillYoutube className="text-[#38a835] scale-[3]" />
                  </button>
                )}
              </div>
            )
          })}
        </div>

        <div className="mb-[-3rem] gap-[40px] md:hidden flex">
          {topAnimes.slice(0, 2).map((anime) => {
            const isSelected = selectedAnime?.id === anime.id
            return (
              <div
                key={anime.id}
                onClick={() => setSelectedAnime(anime)}
                style={{
                  backgroundImage: `url(${anime.imgUrl})`,
                  backgroundSize: 'cover',
                  backgroundPosition: 'center',
                  backgroundRepeat: 'no-repeat',
                }}
                className={`w-[200px] flex items-center justify-center cursor-pointer transition-all duration-300 ${
                  isSelected ? 'border-white border-[2px] scale-110' : 'opacity-50'
                } h-[120px] rounded-[20px]`}
              >
                {isSelected && (
                  <button
                    type="button"
                    aria-label={`View ${anime.title}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      router.push(`/info/${anime.id}`)
                    }}
                    className="bg-white scale-110 transition-all duration-300"
                  >
                    <AiFillYoutube className="text-[#38a835] scale-[3]" />
                  </button>
                )}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

export default Hero
