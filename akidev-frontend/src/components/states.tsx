import { cn } from "@/lib/utils";

import { Button } from "./button";

export function SectionTitle({ children, className }: { children: React.ReactNode; className?: string }) {
  return <h2 className={cn("text-xl font-bold text-ink", className)}>{children}</h2>;
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

function Centred({ title, detail, tone, children }: { title: string; detail: string; tone: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-8 py-16 text-center">
      <p className={cn("text-lg font-bold", tone)}>{title}</p>
      <p className="max-w-[520px] text-sm text-dim">{detail}</p>
      {children}
    </div>
  );
}

/** Nothing to show, and nothing went wrong. */
export function EmptyState({ title, detail }: { title: string; detail: string }) {
  return <Centred title={title} detail={detail} tone="text-ink" />;
}

/** Something went wrong, with a way to try again. */
export function ErrorState({ title, detail, onRetry }: { title: string; detail: string; onRetry?: () => void }) {
  return (
    <Centred title={title} detail={detail} tone="text-danger">
      {onRetry && (
        <Button look="secondary" className="mt-3" onClick={onRetry}>
          Try again
        </Button>
      )}
    </Centred>
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
