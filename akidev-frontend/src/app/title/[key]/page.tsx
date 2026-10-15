import type { Metadata } from "next";

import { stripTags } from "@/lib/text";
import TitleView from "@/components/title-view";
import { serverGet } from "@/lib/api-server";
import type { Detail } from "@/types/media";

// Rendered for each request with the title already in it; see app/page.tsx. The data is kept
// longer than a catalogue's, because a title changes slowly.
export const dynamic = "force-dynamic";

const detail = (key: string) => serverGet<Detail>(`/v1/titles/${encodeURIComponent(key)}`, 300);

export async function generateMetadata({ params }: { params: Promise<{ key: string }> }): Promise<Metadata> {
  const data = await detail((await params).key);
  if (!data) return {};
  const { title, description, banner, poster } = data.media;
  const text = description ? stripTags(description).slice(0, 200) : undefined;
  const image = banner ?? poster ?? undefined;
  return {
    title: `${title} | Akidev`,
    description: text,
    openGraph: { title, description: text, images: image ? [image] : undefined },
  };
}

export default async function Page({ params }: { params: Promise<{ key: string }> }) {
  const { key } = await params;
  const data = await detail(key);
  return <TitleView id={key} initial={data ?? undefined} />;
}
