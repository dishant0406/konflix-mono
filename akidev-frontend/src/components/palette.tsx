"use client";

import { Search } from "lucide-react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import { KIND_LABEL } from "@/constants/site";
import { searchPath, titlePath } from "@/constants/routes";
import { facts } from "@/lib/media";
import { cn } from "@/lib/utils";
import { useSearch } from "@/query/use-media";

import { MetaLine } from "./states";

const SHOWN = 8;

/** Search over the page: type, move with the arrows, Enter opens a title or all the results. */
export default function Palette({ onClose }: { onClose: () => void }) {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);
  const [typed, setTyped] = useState("");
  const [term, setTerm] = useState("");
  const [selected, setSelected] = useState(0);
  const { results, stale, searching, failed } = useSearch(term, SHOWN);

  useEffect(() => input.current?.focus(), []);

  // Wait for a pause in typing, so each keystroke is not a request.
  useEffect(() => {
    const timer = setTimeout(() => setTerm(typed.trim()), 200);
    return () => clearTimeout(timer);
  }, [typed]);

  useEffect(() => setSelected(0), [results]);

  // The page behind stays put while the panel is open.
  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  }, []);

  const open = (id: string) => {
    onClose();
    router.push(titlePath(id));
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") onClose();
    else if (event.key === "ArrowDown" && results.length > 0) {
      event.preventDefault();
      setSelected((index) => (index + 1) % results.length);
    } else if (event.key === "ArrowUp" && results.length > 0) {
      event.preventDefault();
      setSelected((index) => (index + results.length - 1) % results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (results[selected] && !stale && term === typed.trim()) open(results[selected].id);
      else if (typed.trim()) {
        onClose();
        router.push(searchPath(typed.trim()));
      }
    }
  };

  const empty = typed.trim().length === 0;
  const hint = empty
    ? "Type to search anime, series and films."
    : searching && results.length === 0
      ? "Searching..."
      : failed
        ? "Couldn't reach the catalogue. Check your connection."
        : results.length === 0
          ? "No matches. Press Enter to search everywhere."
          : null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Search"
      className="fixed inset-0 z-50 flex items-start justify-center bg-canvas/70 px-4 pt-[110px] backdrop-blur-sm animate-fade"
      onClick={onClose}
      onKeyDown={onKeyDown}
    >
      <div
        className="flex w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3">
          <Search className="size-[18px] shrink-0 text-dim" />
          <input
            ref={input}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            placeholder="Search"
            aria-label="Search anime, series and films"
            autoComplete="off"
            spellCheck={false}
            className="w-full bg-transparent text-base text-ink outline-none placeholder:text-faint"
          />
        </div>

        {(hint || results.length > 0) && (
          <div className={cn("flex max-h-[60vh] flex-col overflow-y-auto border-t border-line p-2", stale && "opacity-50")}>
            {results.map((card, index) => (
              <button
                key={card.id}
                type="button"
                onClick={() => open(card.id)}
                onMouseMove={() => setSelected(index)}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left",
                  index === selected && "bg-raised",
                )}
              >
                <div className="relative h-[54px] w-9 shrink-0 overflow-hidden rounded-md bg-line">
                  {card.poster && <Image src={card.poster} alt="" fill sizes="36px" className="object-cover" />}
                </div>
                <div className="flex min-w-0 flex-col gap-0.5">
                  <span className="truncate text-[15px] font-semibold text-ink">{card.title}</span>
                  <span className="flex items-center gap-2">
                    <span className="text-[10px] font-bold text-faint">{KIND_LABEL[card.kind]}</span>
                    <MetaLine parts={facts(card)} className="text-xs" />
                  </span>
                </div>
              </button>
            ))}
            {hint && <p className="px-3 py-4 text-sm text-dim">{hint}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
