import Image from "next/image";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { cn, mediaPoster, mediaTitle } from "@/lib/utils";
import type { MediaList } from "@/types/media";

type Props = {
  media: MediaList;
  href: string;
  className?: string;
  /** Hides the gradient and the text, for a plain poster tile. */
  displayDetails?: boolean;
  /** Shown as a badge over the poster, for a "latest episode" row. */
  episodeLabel?: string;
  variant?: "sm" | "lg";
  priority?: boolean;
};

export default function AnimeCard({
  media,
  href,
  className,
  displayDetails = true,
  episodeLabel,
  variant = "sm",
  priority = false,
}: Props) {
  const title = mediaTitle(media.title);
  const poster = mediaPoster(media.coverImage);

  return (
    <Link
      href={href}
      className={cn(
        "group relative block overflow-hidden rounded-xl cursor-pointer",
        "transition-transform duration-300 hover:scale-105 focus-visible:scale-105",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        variant === "sm" && "h-[12rem] min-[320px]:h-[16.625rem] sm:h-[18rem] max-w-[12.625rem] md:min-w-[12rem]",
        variant === "lg" && "max-w-[12.625rem] md:max-w-[18.75rem] h-auto md:h-[25rem] shrink-0 lg:w-[18.75rem]",
        className,
      )}
    >
      {poster ? (
        <Image
          src={poster}
          alt={title}
          fill
          sizes="(max-width: 640px) 45vw, (max-width: 1024px) 25vw, 200px"
          className="object-cover"
          priority={priority}
        />
      ) : (
        <div className="absolute inset-0 bg-placeholder" />
      )}

      {displayDetails && (
        <>
          <div className="absolute inset-0 bg-gradient-to-t from-background/95 via-background/20 to-transparent" />
          <div className="absolute bottom-0 flex w-full flex-col gap-1 px-4 pb-3">
            <h5 className="line-clamp-1 text-sm font-bold text-foreground">{title}</h5>
            <div className="mt-0.5 flex flex-wrap items-center gap-1.5">
              {episodeLabel && (
                <Badge className="bg-primary px-1.5 py-0.5 text-[10px] text-primary-foreground">{episodeLabel}</Badge>
              )}
              {media.format && (
                <Badge variant="outline" className="border-border px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {media.format}
                </Badge>
              )}
              {!!media.averageScore && (
                <span className="ml-auto text-[11px] font-semibold text-ayu-yellow">★ {media.averageScore}%</span>
              )}
            </div>
          </div>
        </>
      )}
    </Link>
  );
}
