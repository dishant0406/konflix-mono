import { cn } from "@/lib/utils";

import { Button } from "./button";

export function SectionTitle({ children, className }: { children: React.ReactNode; className?: string }) {
  return <h2 className={cn("font-title text-xl font-semibold tracking-[-0.01em] text-ink", className)}>{children}</h2>;
}

/** "2026 · tv · 24 episodes": quiet facts under a title. */
export function MetaLine({ parts, className }: { parts: string[]; className?: string }) {
  return (
    <div className={cn("flex flex-wrap items-center gap-x-2 text-sm text-dim", className)}>
      {parts.map((part, index) => (
        <span key={`${part}-${index}`} className="flex items-center gap-2">
          {index > 0 && <span className="text-faint">·</span>}
          {part}
        </span>
      ))}
    </div>
  );
}

/** A grey block that pulses while something loads. */
export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("animate-pulse rounded-md bg-raised", className)} aria-hidden />;
}

/**
 * A title card: a short ember rule, a small caps line saying what kind of moment this is, one
 * line in the title face, a few quiet words, and the ways forward. Every empty and error screen
 * is this, so they read as part of the film and not as a browser notice.
 */
export function StateCard({
  eyebrow,
  title,
  detail,
  tone = "calm",
  compact = false,
  children,
  className,
}: {
  eyebrow?: string;
  title: string;
  detail?: string;
  tone?: "calm" | "trouble";
  /** For a panel rather than a whole screen. */
  compact?: boolean;
  children?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center px-8 text-center", compact ? "py-12" : "py-20", className)}>
      <span aria-hidden className={cn("mb-5 h-px w-8", tone === "trouble" ? "bg-danger" : "bg-accent")} />
      {eyebrow && <p className={cn("caps mb-3", tone === "trouble" ? "text-danger" : "text-faint")}>{eyebrow}</p>}
      <p className={cn("max-w-[560px] font-title font-semibold leading-tight tracking-[-0.015em] text-ink", compact ? "text-xl" : "text-[28px]")}>{title}</p>
      {detail && <p className="mt-3 max-w-[460px] text-sm leading-relaxed text-dim">{detail}</p>}
      {children && <div className="mt-7 flex flex-wrap items-center justify-center gap-3">{children}</div>}
    </div>
  );
}

/** Nothing to show, and nothing went wrong. */
export function EmptyState({ title, detail }: { title: string; detail: string }) {
  return <StateCard title={title} detail={detail} />;
}

/** Something went wrong, with a way to try again. */
export function ErrorState({
  title,
  detail,
  onRetry,
  children,
}: {
  title: string;
  detail: string;
  onRetry?: () => void;
  /** Other ways forward, shown after the retry. */
  children?: React.ReactNode;
}) {
  return (
    <StateCard eyebrow="Something went wrong" title={title} detail={detail} tone="trouble">
      {onRetry && (
        <Button look="secondary" onClick={onRetry}>
          Try again
        </Button>
      )}
      {children}
    </StateCard>
  );
}

/** A small ring, for the places something is genuinely being waited for (a stream being found). */
export function Spinner({ className }: { className?: string }) {
  return (
    <div
      role="status"
      aria-label="Loading"
      className={cn("size-5 animate-spin rounded-full border-[1.5px] border-ink/15 border-t-ink/70", className)}
    />
  );
}

/** The end of a grid that has more: how many are shown, and one quiet way to see more. */
export function LoadMore({ shown, loading, onMore }: { shown: number; loading: boolean; onMore: () => void }) {
  return (
    <div className="flex flex-col items-center gap-4 pb-6 pt-10">
      <div className="flex w-full max-w-[420px] items-center gap-4" aria-hidden>
        <span className="h-px flex-1 bg-line" />
        <span className="caps text-faint">{shown} shown</span>
        <span className="h-px flex-1 bg-line" />
      </div>
      <Button look="secondary" onClick={onMore} disabled={loading} className="min-w-36">
        {loading ? <Spinner className="size-4" /> : "Show more"}
      </Button>
    </div>
  );
}
