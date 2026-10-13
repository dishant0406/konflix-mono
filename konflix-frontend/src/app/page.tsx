"use client";

import { useMemo } from "react";

import AnimeRow from "@/components/anime-row";
import FeaturedCollection, { type FeaturedCategory } from "@/components/featured-collection";
import HeroSection from "@/components/hero-section";
import { ROUTES } from "@/constants/routes";
import { useMediaList } from "@/query/use-media";
import type { MediaList } from "@/types/media";

const ROW_SIZE = 20;

export default function HomePage() {
  const spotlight = useMediaList({ sort: "trending", perPage: 6 });
  const trending = useMediaList({ sort: "popularity", perPage: ROW_SIZE });
  const recent = useMediaList({ sort: "recent", perPage: ROW_SIZE });
  const topRated = useMediaList({ sort: "score", perPage: ROW_SIZE });
  const action = useMediaList({ genre: "Action", sort: "trending", perPage: ROW_SIZE });
  const romance = useMediaList({ genre: "Romance", sort: "popularity", perPage: ROW_SIZE });

  const results = (page: { results?: MediaList[] | null } | undefined): MediaList[] => page?.results ?? [];

  // The collage needs three distinct posters per card, so the rows it draws from
  // are sliced rather than reused wholesale.
  const featured = useMemo<FeaturedCategory[]>(
    () => [
      { title: "Top Rated", media: results(topRated.data).slice(0, 3) },
      { title: "Trending Now", media: results(trending.data).slice(0, 3) },
      { title: "Recently Added", media: results(recent.data).slice(0, 3) },
    ],
    [topRated.data, trending.data, recent.data],
  );

  return (
    <div className="flex flex-col pb-10">
      <HeroSection media={results(spotlight.data).slice(0, 6)} loading={spotlight.isLoading} />

      <div className="relative z-20 -mt-8">
        <AnimeRow
          title="Trending Now"
          media={results(trending.data)}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          loading={trending.isLoading}
        />

        <AnimeRow
          title="Newly Released"
          media={results(recent.data)}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          loading={recent.isLoading}
        />

        <FeaturedCollection categories={featured} loading={topRated.isLoading} />

        <AnimeRow
          title="Top Rated"
          media={results(topRated.data)}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          loading={topRated.isLoading}
        />

        <AnimeRow
          title="Action"
          media={results(action.data)}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          loading={action.isLoading}
        />

        <AnimeRow
          title="Romance"
          media={results(romance.data)}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          loading={romance.isLoading}
        />
      </div>
    </div>
  );
}
