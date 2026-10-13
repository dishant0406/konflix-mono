import Link from "next/link";
import Image from "next/image";

import Container from "@/components/container";
import { ROUTES } from "@/constants/routes";
import { mediaPoster, mediaTitle } from "@/lib/utils";
import type { MediaList } from "@/types/media";

export interface FeaturedCategory {
  title: string;
  media: MediaList[];
}

/**
 * The three-poster collage, which is the most recognisable part of the layout.
 * Each category needs at least three entries; with fewer the fanned posters
 * overlap into a single unreadable pile, so it is skipped instead.
 */
export default function FeaturedCollection({
  categories,
  loading,
}: {
  categories: FeaturedCategory[];
  loading: boolean;
}) {
  const usable = categories.filter((category) => category.media.length >= 3);
  if (loading || !usable.length) return <CollectionSkeleton />;

  return (
    <Container className="flex flex-col items-center gap-5 py-5 lg:items-start">
      <h2 className="text-xl font-bold text-white md:text-2xl">Featured Collection</h2>
      <div className="grid w-full grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3">
        {usable.map((category) => (
          <FeaturedCard key={category.title} category={category} />
        ))}
      </div>
    </Container>
  );
}

function FeaturedCard({ category }: { category: FeaturedCategory }) {
  const positions = [
    "left-[15%] rotate-[-20deg] bottom-[-4.25rem] md:bottom-[-5.25rem]",
    "left-[30%] rotate-[-10deg] bottom-[-5rem] md:bottom-[-6.25rem]",
    "left-[45%] rotate-[5deg] bottom-[-6rem] md:bottom-[-6.25rem]",
  ];

  return (
    <div className="flex h-[18.5rem] w-full flex-col items-center gap-2 overflow-hidden rounded-lg bg-[#212121]">
      <h3 className="pt-5 text-center text-lg font-semibold text-white">{category.title}</h3>
      <div className="relative flex w-full grow">
        {category.media.slice(0, 3).map((item, index) => {
          const poster = mediaPoster(item.coverImage);
          if (!poster) return null;
          return (
            <Link
              key={`${item.id}-${index}`}
              href={`${ROUTES.ANIME_DETAILS}/${item.id}`}
              aria-label={mediaTitle(item.title)}
              className={`absolute w-[9.375rem] overflow-hidden rounded-md border-[0.5rem] border-[#212121] transition-transform duration-300 hover:z-10 hover:scale-105 ${positions[index]}`}
            >
              <Image
                src={poster}
                alt={mediaTitle(item.title)}
                width={150}
                height={225}
                className="h-[13.125rem] w-full object-cover"
              />
            </Link>
          );
        })}
      </div>
    </div>
  );
}

function CollectionSkeleton() {
  return (
    <Container className="flex flex-col items-center gap-5 py-10 lg:items-start">
      <div className="h-10 w-[15.625rem] animate-pulse bg-slate-800" />
      <div className="grid w-full grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3">
        {[0, 1, 2].map((index) => (
          <div key={index} className="h-[15.625rem] w-full animate-pulse rounded-xl bg-slate-800 md:h-[18.75rem]" />
        ))}
      </div>
    </Container>
  );
}
