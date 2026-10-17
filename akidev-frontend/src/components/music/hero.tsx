import { cn } from "@/lib/utils";

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
        style={{ background: `linear-gradient(to bottom, rgb(${tint} / 0.55), rgb(11 11 13))` }}
      />
      <div className="relative flex flex-col items-start gap-5 px-4 pb-6 pt-24 sm:flex-row sm:items-end md:gap-6 md:px-8">
        <Cover
          src={cover}
          sizes="232px"
          round={round}
          priority
          className={cn("size-36 shrink-0 shadow-[0_8px_24px_rgb(0_0_0/0.5)] md:size-[232px]", round && "shadow-[0_8px_24px_rgb(0_0_0/0.5)]")}
        />
        <div className="flex min-w-0 flex-col gap-2">
          <p className="text-[13px] font-semibold">{kind}</p>
          <h1 className="text-[clamp(2rem,6vw,4.5rem)] font-extrabold leading-[1.02] tracking-[-0.03em]">{title}</h1>
          {children}
        </div>
      </div>
    </div>
  );
}
