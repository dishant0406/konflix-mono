"use client";

import Hero from "@/components/hero";
import MediaRow from "@/components/row";
import { watchPath } from "@/constants/routes";
import { fraction, useRecent } from "@/lib/progress";
import { useFeatured, useHomeRows } from "@/query/use-media";

export default function HomePage() {
  const featured = useFeatured();
  const rows = useHomeRows();
  const recent = useRecent();

  return (
    <div className="pb-8">
      <Hero items={featured.items} loading={featured.loading} />

      {recent.length > 0 && (
        <MediaRow
          title="Continue watching"
          items={recent.map((entry) => entry.media)}
          resume={recent.map((entry) => ({
            fraction: fraction(entry),
            href: watchPath(entry.media.id, entry.episodeId),
          }))}
        />
      )}

      {rows.map((row) => (
        <MediaRow key={row.title} title={row.title} items={row.items} loading={row.loading} />
      ))}
    </div>
  );
}

