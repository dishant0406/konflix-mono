// Reads the video id out of any YouTube URL shape.
//
// The previous version only matched "watch?v=" and "/embed/", so youtu.be links
// produced "null" in the hero background image.
export function getYoutubeId(url) {
  if (!url) return null
  const match = /(?:youtu\.be\/|v=|\/embed\/|\/shorts\/|\/v\/)([\w-]{11})/.exec(url)
  return match ? match[1] : null
}

export function youtubeThumbnail(url) {
  const id = getYoutubeId(url)
  return id ? `https://i.ytimg.com/vi_webp/${id}/sddefault.webp` : null
}
