"use client";

import Hero from "@/components/hero";
import MediaRow from "@/components/row";
import { watchPath } from "@/constants/routes";
import { fraction, useRecent } from "@/lib/progress";
import { useHome } from "@/query/use-media";
import type { HomeResponse } from "@/types/media";

/** The rows drawn as placeholders while the page loads. */
const LOADING_ROWS = ["Trending Anime", "New Releases", "Popular Series"];

export default function HomeView({ initial }: { initial?: HomeResponse }) {
  const home = useHome(initial);
  const recent = useRecent();

  return (
    <div className="pb-8">
      <Hero items={home.featured} loading={home.loading} />

      {recent.length > 0 && (
        <MediaRow
          eager
          title="Continue watching"
          items={recent.map((entry) => entry.media)}
          resume={recent.map((entry) => ({
            fraction: fraction(entry),
            href: watchPath(entry.media.id, entry.episodeId),
          }))}
        />
      )}

      {home.loading
        ? LOADING_ROWS.map((title) => <MediaRow key={title} title={title} items={[]} loading />)
        : home.rows.map((row, index) => <MediaRow key={row.title} title={row.title} items={row.items} eager={index < 2} />)}
    </div>
  );
}

