"use client";

import { useQuery } from "react-query";

import { get } from "@/lib/api";
import type { Album, Artist, MusicSearch } from "@/lib/music";

const MINUTE = 60 * 1000;

export function useMusicSearch(term: string) {
  return useQuery(["music", "search", term], () => get<MusicSearch>("/v1/music/search", { q: term }), {
    enabled: term.length > 0,
    staleTime: 10 * MINUTE,
    keepPreviousData: true,
  });
}

export function useAlbum(id: string) {
  return useQuery(["music", "album", id], () => get<Album>(`/v1/music/albums/${encodeURIComponent(id)}`), {
    staleTime: 30 * MINUTE,
  });
}

export function useArtist(id: string) {
  return useQuery(["music", "artist", id], () => get<Artist>(`/v1/music/artists/${encodeURIComponent(id)}`), {
    staleTime: 30 * MINUTE,
  });
}

export function useMusicHome() {
  return useQuery(["music", "home"], () => get<MusicSearch>("/v1/music/home"), { staleTime: 30 * MINUTE });
}
