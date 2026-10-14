"use client";

import { SearchIcon } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef, useState } from "react";

import { Input } from "@/components/ui/input";
import { ROUTES, titlePath } from "@/constants/routes";
import { SITE } from "@/constants/site";
import useDebounce from "@/hooks/use-debounce";
import { useSearchSuggestions } from "@/query/use-search-suggestions";
import { cn, mediaPoster, mediaTitle } from "@/lib/utils";

export default function SearchBar({
  className,
  onNavigate,
}: {
  className?: string;
  onNavigate?: () => void;
}) {
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const debounced = useDebounce(value, 300);
  const containerRef = useRef<HTMLDivElement>(null);
  const router = useRouter();

  const { data: suggestions = [], isFetching } = useSearchSuggestions(debounced);
  const showDropdown = open && value.trim().length > 0;

  const close = () => {
    setOpen(false);
    onNavigate?.();
  };

  const submit = () => {
    if (!value.trim()) return;
    router.push(`${ROUTES.SEARCH}?q=${encodeURIComponent(value.trim())}`);
    setValue("");
    close();
  };

  return (
    <div ref={containerRef} className={cn("relative w-full", className)}>
      <SearchIcon className="pointer-events-none absolute inset-y-0 left-3 my-auto h-4 w-4 text-muted-foreground" />
      <Input
        value={value}
        placeholder={`Search ${SITE.itemLabelPlural}...`}
        aria-label={`Search ${SITE.itemLabelPlural}`}
        className="h-10 w-full border-input bg-secondary/60 pl-9 text-foreground placeholder:text-muted-foreground"
        onChange={(event) => setValue(event.target.value)}
        onFocus={() => setOpen(true)}
        onBlur={() => {
          // The dropdown holds focus while it is clicked, so the close is deferred
          // by a frame rather than closing on blur directly.
          setTimeout(() => setOpen(false), 120);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") submit();
          if (event.key === "Escape") close();
        }}
      />

      {showDropdown && (
        <div className="absolute top-[110%] z-50 flex max-h-[60vh] w-full flex-col gap-1 overflow-y-auto rounded-md border border-border bg-popover p-2 shadow-xl">
          {suggestions.length === 0 && (
            <p className="px-3 py-4 text-sm text-muted-foreground">
              {isFetching ? "Searching..." : "No matches."}
            </p>
          )}

          {suggestions.map((item) => {
            const poster = mediaPoster(item.coverImage);
            return (
              <Link
                key={item.id}
                href={titlePath(item.id)}
                onClick={close}
                className="flex items-center gap-3 rounded-md p-2 hover:bg-secondary"
              >
                <div className="relative h-[4.5rem] w-[3.25rem] shrink-0 overflow-hidden rounded-md bg-placeholder">
                  {poster && (
                    <Image src={poster} alt={mediaTitle(item.title)} fill sizes="60px" className="object-cover" />
                  )}
                </div>
                <div className="min-w-0">
                  <p className="line-clamp-2 text-sm text-foreground">{mediaTitle(item.title)}</p>
                  <p className="text-xs text-muted-foreground">
                    {[item.format, item.seasonYear].filter(Boolean).join(" · ")}
                  </p>
                </div>
              </Link>
            );
          })}

          {suggestions.length > 0 && (
            <button
              type="button"
              onClick={submit}
              className="mt-1 w-full rounded-md bg-primary py-2 text-sm font-semibold text-primary-foreground hover:bg-primary/90"
            >
              Show all results
            </button>
          )}
        </div>
      )}
    </div>
  );
}
