import { X } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

import { IconButton } from "./button";

/**
 * The player's side panel, for the episodes and the sources: one header (title and close) and a
 * body that scrolls. A sheet over the whole screen on a phone, a strip down the right edge, as in the desktop app, elsewhere. The caller's `className` moves it, for the places with no control bar.
 */
export function SidePanel({
  title,
  onClose,
  className,
  children,
}: {
  title: string;
  onClose: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <aside
      aria-label={title}
      className={cn(
        "material-panel absolute inset-0 z-10 flex animate-slide flex-col overflow-hidden sm:left-auto sm:w-[400px] sm:shadow-[-1px_0_0_rgb(var(--ink)/0.08),-24px_0_48px_rgb(0_0_0/0.35)]",
        className,
      )}
    >
      <header className="flex h-14 shrink-0 items-center justify-between pl-5 pr-3 shadow-[0_1px_0_rgb(var(--ink)/0.07)]">
        <h2 className="font-title text-lg font-semibold tracking-[-0.01em] text-ink">{title}</h2>
        <IconButton label={`Close ${title.toLowerCase()}`} onClick={onClose} className="size-8">
          <X className="size-4" />
        </IconButton>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">{children}</div>
    </aside>
  );
}
