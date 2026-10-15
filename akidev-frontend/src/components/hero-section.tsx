"use client";

import { ArrowLeft, ArrowRight } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useState } from "react";

import Container from "@/components/container";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Carousel,
  CarouselContent,
  CarouselItem,
  type CarouselApi,
} from "@/components/ui/carousel";
import { titlePath } from "@/constants/routes";
import { mediaBanner, mediaTitle } from "@/lib/utils";
import type { MediaList } from "@/types/media";

export default function HeroSection({ media, loading }: { media: MediaList[]; loading: boolean }) {
  const [api, setApi] = useState<CarouselApi>();

  if (loading || !media.length) return <HeroSkeleton />;

  return (
    <section className="relative h-[70vh] min-h-[26rem] w-full md:h-[80vh]">
      <Carousel className="w-full" setApi={setApi} opts={{ loop: true }}>
        <CarouselContent>
          {media.map((item, index) => (
            <CarouselItem key={item.id ?? index}>
              <HeroSlide media={item} priority={index === 0} />
            </CarouselItem>
          ))}
        </CarouselContent>
      </Carousel>

      <div className="absolute bottom-10 right-6 z-30 hidden items-center gap-3 md:flex lg:right-10">
        <Button
          variant="outline"
          size="icon"
          aria-label="Previous featured title"
          onClick={() => api?.scrollPrev()}
          className="h-10 w-10 rounded-full border-input bg-background/60 backdrop-blur hover:bg-secondary"
        >
          <ArrowLeft className="h-4 w-4 text-foreground" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          aria-label="Next featured title"
          onClick={() => api?.scrollNext()}
          className="h-10 w-10 rounded-full border-input bg-background/60 backdrop-blur hover:bg-secondary"
        >
          <ArrowRight className="h-4 w-4 text-foreground" />
        </Button>
      </div>
    </section>
  );
}

function HeroSlide({ media, priority }: { media: MediaList; priority: boolean }) {
  const title = mediaTitle(media.title);
  const image = mediaBanner(media);
  const studios = (media.studios?.nodes ?? []).map((studio) => studio.name).filter(Boolean);

  return (
    <div className="relative h-[70vh] min-h-[26rem] w-full overflow-hidden md:h-[80vh]">
      {image ? (
        <Image
          src={image}
          alt={title}
          fill
          sizes="100vw"
          priority={priority}
          className="object-cover"
        />
      ) : (
        <div className="absolute inset-0 bg-card" />
      )}

      {/* Two scrims: one horizontal so the copy stays readable over a busy image,
          one vertical so the carousel meets the page without a hard edge. */}
      <div className="absolute inset-0 bg-gradient-to-r from-background via-background/80 to-transparent" />
      <div className="absolute inset-0 bg-gradient-to-t from-background via-background/30 to-transparent" />

      <div className="relative z-20 h-full">
        <Container className="flex h-full flex-col justify-end pb-12 md:justify-center">
          <div className="space-y-3 lg:w-[45vw]">
            <h1 className="line-clamp-2 text-3xl font-black text-foreground md:text-5xl">{title}</h1>

            <div className="flex flex-wrap items-center gap-2 text-xs">
              {media.format && <Badge className="bg-primary font-bold text-primary-foreground">{media.format}</Badge>}
              {media.seasonYear && (
                <Badge variant="outline" className="border-border text-muted-foreground">
                  {media.season ? `${media.season} ` : ""}
                  {media.seasonYear}
                </Badge>
              )}
              {!!media.averageScore && (
                <Badge className="border border-ayu-yellow/40 bg-ayu-yellow/15 text-ayu-yellow">
                  ★ {media.averageScore}% score
                </Badge>
              )}
              {(media.genres ?? []).slice(0, 3).map((genre) => (
                <span key={genre} className="text-xs text-muted-foreground">
                  • {genre}
                </span>
              ))}
            </div>

            {studios.length > 0 && (
              <p className="line-clamp-1 text-xs text-muted-foreground">{studios.join(", ")}</p>
            )}

            <div className="flex items-center gap-3 pt-3">
              <Button asChild className="h-10 bg-primary text-primary-foreground hover:bg-primary/90">
                <Link href={titlePath(media.id)}>Learn More</Link>
              </Button>
            </div>
          </div>
        </Container>
      </div>
    </div>
  );
}

function HeroSkeleton() {
  return (
    <section className="relative h-[70vh] min-h-[26rem] w-full md:h-[80vh]">
      <div className="absolute inset-0 animate-pulse bg-placeholder" />
      <Container className="relative z-20 flex h-full flex-col justify-end pb-12 md:justify-center">
        <div className="space-y-4 lg:w-[45vw]">
          <div className="h-12 w-3/4 animate-pulse rounded bg-secondary" />
          <div className="h-6 w-1/2 animate-pulse rounded bg-secondary" />
          <div className="h-10 w-32 animate-pulse rounded bg-secondary" />
        </div>
      </Container>
    </section>
  );
}
