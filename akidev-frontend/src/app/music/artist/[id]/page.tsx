import ArtistView from "@/components/music/artist-view";

export default async function ArtistPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <ArtistView id={decodeURIComponent(id)} />;
}
