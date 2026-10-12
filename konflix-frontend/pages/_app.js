import '../styles/globals.css'
import 'slick-carousel/slick/slick.css'
import 'slick-carousel/slick/slick-theme.css'
import 'plyr-react/plyr.css'

import { useEffect, useState } from 'react'
import Image from 'next/image'
import Router from 'next/router'

import { SEO } from '@/components'

export default function App({ Component, pageProps }) {
  const [loading, setLoading] = useState(false)

  // Client-side navigation fetches data before rendering, which can take a moment
  // on the pages that resolve a stream, so the transition is shown explicitly.
  useEffect(() => {
    const start = () => setLoading(true)
    const end = () => setLoading(false)

    Router.events.on('routeChangeStart', start)
    Router.events.on('routeChangeComplete', end)
    Router.events.on('routeChangeError', end)
    return () => {
      Router.events.off('routeChangeStart', start)
      Router.events.off('routeChangeComplete', end)
      Router.events.off('routeChangeError', end)
    }
  }, [])

  return (
    <div className="relative">
      {loading && (
        <div className="fixed z-[100] top-[45%] right-[45%]">
          <Image src="/Images/loading.webp" alt="" width={106} height={100} />
        </div>
      )}
      <SEO />
      <div className={loading ? 'opacity-70' : undefined}>
        <Component {...pageProps} />
      </div>
    </div>
  )
}
