import { cn } from "@/lib/utils";

import Bone from "./bone";
import Cover from "./cover";

/**
 * The top of an album or artist page: the cover beside a large title, over a wash of the cover's
 * own colour that fades into the page. The wash starts behind the search bar.
 */
export default function Hero({
  tint,
  cover,
  round,
  kind,
  title,
  children,
}: {
  tint: string;
  cover: string | null;
  round?: boolean;
  kind: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="relative">
      <div
        aria-hidden
        className="absolute inset-x-0 top-0 h-[26rem] transition-colors duration-300"
        style={{ background: `linear-gradient(to bottom, rgb(${tint} / 0.55), rgb(var(--canvas)))` }}
      />
      <div className="relative flex flex-col items-start gap-5 gutter pb-6 pt-24 sm:flex-row sm:items-end md:gap-6">
        <Cover
          src={cover}
          sizes="232px"
          round={round}
          priority
          className={cn("size-36 shrink-0 shadow-[0_8px_24px_rgb(0_0_0/0.5)] md:size-[232px]", round && "shadow-[0_8px_24px_rgb(0_0_0/0.5)]")}
        />
        <div className="flex min-w-0 flex-col gap-2">
          <p className="caps text-white/70">{kind}</p>
          <h1 className="font-title text-[clamp(2rem,6vw,4.5rem)] font-semibold leading-[1.1] tracking-[-0.02em]">{title}</h1>
          {children}
        </div>
      </div>
    </div>
  );
}

/**
 * The hero before the page arrives: the same wash, paddings, cover and rows of text, in grey. The
 * title is one line tall, which is what most are.
 */
export function HeroSkeleton({ round }: { round?: boolean }) {
  return (
    <div className="relative" aria-hidden>
      <div
        className="absolute inset-x-0 top-0 h-[26rem]"
        style={{ background: "linear-gradient(to bottom, rgb(70 70 70 / 0.55), rgb(var(--canvas)))" }}
      />
      <div className="relative flex flex-col items-start gap-5 gutter pb-6 pt-24 sm:flex-row sm:items-end md:gap-6">
        <Bone className={cn("size-36 shrink-0 md:size-[232px]", round && "rounded-full")} />
        <div className="flex min-w-0 flex-col gap-2">
          <div className="flex h-5 items-center">
            <Bone className="h-3 w-12" />
          </div>
          <Bone className="w-[min(32rem,80vw)]" style={{ height: "calc(clamp(2rem, 6vw, 4.5rem) * 1.1)" }} />
          <div className="flex h-5 items-center">
            <Bone className="h-3.5 w-64" />
          </div>
        </div>
      </div>
    </div>
  );
}
