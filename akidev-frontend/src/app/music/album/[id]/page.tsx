import AlbumView from "@/components/music/album-view";

export default async function AlbumPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <AlbumView id={decodeURIComponent(id)} />;
}
