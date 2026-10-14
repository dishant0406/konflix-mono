"use client";

import { MenuIcon, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import Container from "@/components/container";
import SearchBar from "@/components/search-bar";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetClose, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { ROUTES } from "@/constants/routes";
import { useSite } from "@/providers/site-provider";
import useScrollPosition from "@/hooks/use-scroll-position";
import { cn } from "@/lib/utils";

const LINKS = [
  { title: "Home", href: ROUTES.HOME },
  { title: "Browse", href: `${ROUTES.SEARCH}?sort=popularity` },
];

export default function Navbar() {
  const site = useSite();
  const y = useScrollPosition();
  const [menuOpen, setMenuOpen] = useState(false);
  const solid = y > 16;

  return (
    <header
      className={cn(
        "fixed inset-x-0 top-0 z-[100] transition-colors duration-300",
        solid ? "bg-background/85 backdrop-blur-md" : "bg-gradient-to-b from-background to-transparent",
      )}
    >
      <Container className="flex items-center justify-between gap-6 py-3">
        <Link href={ROUTES.HOME} className="shrink-0" aria-label={`${site.appName} home`}>
          <span className="bg-gradient-to-r from-foreground to-primary bg-clip-text text-xl font-black tracking-widest text-transparent">
            KONFLIX
          </span>
        </Link>

        <nav className="ml-6 hidden items-center gap-8 lg:flex">
          {LINKS.map((link) => (
            <Link key={link.title} href={link.href} className="text-sm text-muted-foreground hover:text-foreground">
              {link.title}
            </Link>
          ))}
        </nav>

        <div className="hidden w-1/3 max-w-md lg:block">
          <SearchBar />
        </div>

        <div className="lg:hidden">
          <Sheet open={menuOpen} onOpenChange={setMenuOpen}>
            <SheetTrigger aria-label="Open menu" className="text-foreground">
              <MenuIcon />
            </SheetTrigger>
            <SheetContent
              className="z-[150] flex w-[80vw] flex-col"
              hideCloseButton
              onOpenAutoFocus={(event) => event.preventDefault()}
            >
              <div className="relative h-full w-full">
                <SheetClose className="absolute right-0 top-0 text-foreground" aria-label="Close menu">
                  <X />
                </SheetClose>
                <div className="mt-10 flex flex-col gap-5">
                  {LINKS.map((link) => (
                    <Link
                      key={link.title}
                      href={link.href}
                      onClick={() => setMenuOpen(false)}
                      className="text-foreground"
                    >
                      {link.title}
                    </Link>
                  ))}
                  <Separator />
                  <SearchBar onNavigate={() => setMenuOpen(false)} />
                </div>
              </div>
            </SheetContent>
          </Sheet>
        </div>
      </Container>
    </header>
  );
}
