"use client";

import { useMemo } from "react";

import AnimeRow from "@/components/anime-row";
import FeaturedCollection, { type FeaturedCategory } from "@/components/featured-collection";
import HeroSection from "@/components/hero-section";
import { titlePath } from "@/constants/routes";
import { useSite } from "@/providers/site-provider";
import { useMediaList, useMediaRows } from "@/query/use-media";
import type { MediaList } from "@/types/media";

const ROW_SIZE = 20;

export default function HomePage() {
  const site = useSite();
  // The rows are declared in the site config, so the two sites differ by data rather
  // than by conditionals here. Each row is one query; the config supplies the sort and
  // the genre.
  const rows = useMediaRows(site.rows, ROW_SIZE);

  const spotlight = useMediaList({ sort: "trending", perPage: 6 });

  const results = (page: { results?: MediaList[] | null } | undefined): MediaList[] => page?.results ?? [];

  // The collage needs three distinct posters per card, so the rows it draws from
  // are sliced rather than reused wholesale.
  const featured = useMemo<FeaturedCategory[]>(() => {
    const usable = rows.map((row, index) => ({
      title: site.rows[index].title,
      media: results(row.data),
    }));
    return usable.filter((entry) => entry.media.length >= 3).slice(0, 3);
  }, [rows]);

  // A row whose source is having a bad day renders as a heading with nothing under
  // it, which reads as a broken page. Empty rows are dropped instead, so one failing
  // provider cannot empty the whole site.
  const filled = site.rows.map((row, index) => ({ row, query: rows[index] })).filter(
    ({ query }) => query.isLoading || results(query.data).length > 0,
  );

  return (
    <div className="flex flex-col pb-10">
      <HeroSection media={results(spotlight.data).slice(0, 6)} loading={spotlight.isLoading} />

      <div className="relative z-20 -mt-8">
        {filled.slice(0, 2).map(({ row, query }) => (
          <AnimeRow
            key={row.title}
            title={row.title}
            media={results(query.data)}
            hrefFor={(item) => titlePath(item.id)}
            loading={query.isLoading}
          />
        ))}

        <FeaturedCollection categories={featured} loading={rows[0]?.isLoading ?? false} />

        {filled.slice(2).map(({ row, query }) => (
          <AnimeRow
            key={row.title}
            title={row.title}
            media={results(query.data)}
            hrefFor={(item) => titlePath(item.id)}
            loading={query.isLoading}
          />
        ))}
      </div>
    </div>
  );
}
