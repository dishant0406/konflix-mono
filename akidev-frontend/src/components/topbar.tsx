"use client";

import { ChevronLeft, Search } from "lucide-react";
import dynamic from "next/dynamic";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState } from "react";

import { CATALOGUES } from "@/constants/site";
import { ROUTES } from "@/constants/routes";
import { cn } from "@/lib/utils";

import { IconButton } from "./button";
// Search is loaded the first time it is opened, not with every page.
const Palette = dynamic(() => import("./palette"));

/** The bar turns solid once the page has moved this far. */
const SOLID_AFTER = 40;

const TABS = [{ label: "Home", href: ROUTES.HOME }, ...CATALOGUES.map((page) => ({ label: page.label, href: `/${page.slug}` }))];

/** Home, the three catalogues and search, in one bar that floats over the artwork. */
export default function TopBar() {
  const pathname = usePathname();
  const router = useRouter();
  const [scrolled, setScrolled] = useState(false);
  const [searching, setSearching] = useState(false);
  const [canGoBack, setCanGoBack] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > SOLID_AFTER);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  // Read after each navigation: a page opened straight from a link has nothing behind it.
  useEffect(() => setCanGoBack(window.history.length > 1 && pathname !== ROUTES.HOME), [pathname]);

  // Cmd-K or Ctrl-K opens search from anywhere, as it does in the app.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearching((open) => !open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const active = (href: string) => (href === ROUTES.HOME ? pathname === ROUTES.HOME : pathname === href);

  return (
    <>
      <header
        className={cn(
          "fixed inset-x-0 top-0 z-40 h-14 border-b bg-origin-border transition-colors duration-200",
          scrolled ? "border-line bg-canvas" : "border-transparent bg-gradient-to-b from-canvas/85 to-transparent",
        )}
      >
        <div className="gutter flex h-full items-center gap-3 md:gap-6">
          <IconButton
            label="Back"
            onClick={() => (canGoBack ? router.back() : router.push(ROUTES.HOME))}
            className={cn(!canGoBack && "opacity-35")}
          >
            <ChevronLeft className="size-5" />
          </IconButton>

          <Link href={ROUTES.HOME} className="flex items-center gap-2 text-xl font-black text-ink">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src="/icon.svg" alt="" width={28} height={28} className="size-7 shrink-0" />
            <span className="hidden sm:inline">AKIDEV</span>
          </Link>

          <nav aria-label="Catalogues" className="no-scrollbar flex min-w-0 flex-1 items-center gap-4 overflow-x-auto md:gap-5">
            {TABS.map((tab) => (
              <Link
                key={tab.href}
                href={tab.href}
                // Fetched ahead, so a tab opens the moment it is pressed.
                prefetch
                aria-current={active(tab.href) ? "page" : undefined}
                className={cn(
                  "flex flex-col items-center gap-1 px-1 py-1 text-sm transition-colors",
                  active(tab.href) ? "font-bold text-ink" : "text-dim hover:text-ink",
                )}
              >
                {tab.label}
                <span className={cn("h-0.5 w-full rounded-full", active(tab.href) && "bg-accent")} />
              </Link>
            ))}
          </nav>

          <IconButton label="Search" onClick={() => setSearching(true)} className={cn(searching && "bg-ink/[0.14]")}>
            <Search className="size-[18px]" />
          </IconButton>
        </div>
      </header>

      {searching && <Palette onClose={() => setSearching(false)} />}
    </>
  );
}
