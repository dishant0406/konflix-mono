"use client";

import { Check, ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";

import { byLanguage } from "@/lib/subtitles";
import { cn } from "@/lib/utils";
import type { SubtitleOption } from "@/types/media";

/**
 * The body of the subtitle panel, laid out as the desktop app's: Off, then one row per language
 * that opens to its files. `active` is the file showing, if any.
 */
export default function CaptionsMenu({
  subtitles,
  active,
  onChoose,
}: {
  subtitles: SubtitleOption[];
  active: SubtitleOption | null;
  onChoose: (option: SubtitleOption | null) => void;
}) {
  const languages = useMemo(() => byLanguage(subtitles), [subtitles]);
  /** The language whose files are listed. */
  const [opened, setOpened] = useState<string | null>(active?.lang ?? null);

  return (
    <div role="menu" className="flex flex-col pb-3">
      <div className="px-2 pt-1">
        <Row selected={active === null} onClick={() => onChoose(null)}>
          <span className="text-sm font-semibold text-ink">Off</span>
        </Row>
      </div>
      <p className="px-4 pb-1 pt-3 text-[11px] font-semibold uppercase text-faint">Online</p>
      <div className="flex flex-col gap-1 px-2">
        {languages.map((each) => {
          const open = opened === each.lang;
          return (
            <div key={each.lang} className="flex flex-col gap-1">
              <Row
                selected={false}
                marked={active?.lang === each.lang}
                onClick={() => setOpened(open ? null : each.lang)}
              >
                <span className="flex-1 text-sm font-semibold text-ink">{each.language}</span>
                <span className="text-xs text-dim">{each.options.length}</span>
                {open ? <ChevronDown className="size-3.5 text-dim" /> : <ChevronRight className="size-3.5 text-dim" />}
              </Row>
              {open && (
                <div className="flex flex-col gap-1 pl-6">
                  {each.options.map((option) => {
                    const shown = option.id === active?.id;
                    return (
                      <Row key={option.id} selected={shown} onClick={() => onChoose(option)}>
                        <span className="flex min-w-0 flex-1 flex-col">
                          <span className="truncate text-[13px] text-ink" title={option.label || option.provider}>
                            {option.label || option.provider}
                          </span>
                          <span className="text-[11px] text-faint">{option.provider}</span>
                        </span>
                        {option.hearing_impaired && (
                          <span
                            title="For the deaf and hard of hearing"
                            className="rounded-md bg-wash px-1.5 py-0.5 text-[11px] font-semibold text-dim"
                          >
                            SDH
                          </span>
                        )}
                      </Row>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** A clickable row with a fixed slot for the check mark, so rows line up whether marked or not. */
function Row({
  selected,
  marked = selected,
  onClick,
  children,
}: {
  selected: boolean;
  marked?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onClick}
      className={cn(
        "flex w-full cursor-pointer items-center gap-3 rounded-lg px-2 py-2.5 text-left transition-colors hover:bg-wash",
        selected && "bg-wash",
      )}
    >
      <span className="flex w-4 shrink-0">{marked && <Check className="size-3.5 text-accent" />}</span>
      {children}
    </button>
  );
}
