import Head from 'next/head'

// Site-wide defaults, rendered once from _app.js. Pages that need their own title
// or description render their own <Head>, which Next merges with this one.
export default function SEO({
  title = 'Konflix and Chill: Where Anime Lovers Unite!',
  description = 'Browse and watch anime. Metadata from AniList and Kitsu, video from public torrent indexers.',
}) {
  return (
    <Head>
      <meta name="viewport" content="width=device-width, initial-scale=1.0" />
      <meta charSet="utf-8" />
      <link rel="icon" href="/Images/favicon.ico" />
      <link rel="preconnect" href="https://fonts.googleapis.com" />
      <link rel="preconnect" href="https://fonts.gstatic.com" />

      <title>{title}</title>
      <meta name="description" content={description} />

      {/* Open Graph and Twitter read these `property` attributes; the previous
          version emitted them as `name`, so social previews never picked them up. */}
      <meta property="og:title" content={title} />
      <meta property="og:description" content={description} />
      <meta property="og:type" content="website" />
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:title" content={title} />
      <meta name="twitter:description" content={description} />
    </Head>
  )
}
