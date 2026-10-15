"use client";

import { ChevronLeft } from "lucide-react";
import { useMemo, useState } from "react";

import { byLanguage, optionLine } from "@/lib/subtitles";
import { cn } from "@/lib/utils";
import type { SubtitleOption } from "@/types/media";

/**
 * The popover over the captions button: Off and one row per language; a language with several
 * files opens a second level to choose among them. `active` is the file showing, if any.
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
  /** The language whose files are listed, or null for the top level. */
  const [opened, setOpened] = useState<string | null>(null);
  const language = languages.find((each) => each.lang === opened);

  return (
    <div
      role="menu"
      className="absolute bottom-full right-0 z-20 mb-2 flex max-h-[min(320px,50vh)] w-[min(320px,calc(100vw-2rem))] flex-col overflow-y-auto rounded-xl border border-line bg-surface/[0.97] p-1"
    >
      {language ? (
        <>
          <button type="button" onClick={() => setOpened(null)} className={cn(ROW, "text-dim")}>
            <ChevronLeft className="size-4 shrink-0" />
            Back
          </button>
          {language.options.map((option) => (
            <Row key={option.id} selected={option.id === active?.id} onClick={() => onChoose(option)}>
              <span className="line-clamp-2 min-w-0 flex-1 break-all" title={optionLine(option)}>
                {optionLine(option)}
              </span>
              {option.hearing_impaired && (
                <span title="For the deaf and hard of hearing" className="shrink-0 rounded bg-ink/[0.08] px-1 text-[10px] font-semibold text-dim">
                  SDH
                </span>
              )}
            </Row>
          ))}
        </>
      ) : (
        <>
          <Row selected={active === null} onClick={() => onChoose(null)}>
            Off
          </Row>
          {languages.map((each) => (
            <Row
              key={each.lang}
              selected={active?.lang === each.lang}
              // One file is the choice itself; several are a second step.
              onClick={() => (each.options.length === 1 ? onChoose(each.options[0]) : setOpened(each.lang))}
            >
              <span className="min-w-0 flex-1 truncate">{each.language}</span>
              {each.options.length > 1 && <span className="shrink-0 text-xs text-faint">{each.options.length}</span>}
            </Row>
          ))}
        </>
      )}
    </div>
  );
}

const ROW =
  "flex w-full cursor-pointer items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:bg-ink/[0.08]";

function Row({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onClick}
      className={cn(ROW, selected ? "font-semibold text-ink" : "text-dim")}
    >
      <span className={cn("size-[5px] shrink-0 rounded-full", selected && "bg-accent")} />
      {children}
    </button>
  );
}
