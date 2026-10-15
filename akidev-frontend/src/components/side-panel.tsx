import { X } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

import { IconButton } from "./button";

/**
 * The player's side panel, for the episodes and the sources: one header (title and close) and a
 * body that scrolls. A sheet over the whole screen on a phone, a card at the right edge above the
 * control bar elsewhere. The caller's `className` moves it, for the places with no control bar.
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
        "absolute inset-0 z-10 flex flex-col overflow-hidden bg-surface sm:inset-auto sm:bottom-28 sm:right-4 sm:top-4 sm:w-[380px] sm:rounded-2xl sm:border sm:border-line sm:shadow-2xl sm:shadow-black/50",
        className,
      )}
    >
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-line pl-5 pr-3">
        <h2 className="text-[15px] font-semibold text-ink">{title}</h2>
        <IconButton label={`Close ${title.toLowerCase()}`} onClick={onClose} className="size-8">
          <X className="size-4" />
        </IconButton>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">{children}</div>
    </aside>
  );
}
