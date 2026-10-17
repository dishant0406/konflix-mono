import type { Metadata, Viewport } from "next";
import "./globals.css";

import Footer from "@/components/footer";
import PartyFollower from "@/components/party-follower";
import PlayerBar from "@/components/music/player-bar";
import TopBar from "@/components/topbar";
import { Toaster } from "@/components/ui/sonner";
import { SITE_DOMAIN } from "@/constants/site";
import { appFont } from "@/lib/fonts";
import MusicPlayerProvider from "@/providers/music-player";
import PartyProvider from "@/providers/party";
import QueryProvider from "@/providers/query-provider";

// The Akidev mark: the vector for browsers that take one, the .ico for those that do not, and
// a 180px PNG for a home screen.
const TITLE = "Akidev | Anime, series and films";
const DESCRIPTION = "Browse and stream anime, series and films from one place.";

export const metadata: Metadata = {
  applicationName: "Akidev",
  metadataBase: new URL(`https://${SITE_DOMAIN}`),
  title: TITLE,
  description: DESCRIPTION,
  icons: {
    icon: [
      { url: "/icon.svg", type: "image/svg+xml" },
      { url: "/favicon.ico", sizes: "any" },
    ],
    apple: "/apple-touch-icon.png",
  },
  openGraph: { type: "website", siteName: "Akidev", title: TITLE, description: DESCRIPTION },
  twitter: { card: "summary", title: TITLE, description: DESCRIPTION },
};

export const viewport: Viewport = { themeColor: "#0b0b0d", colorScheme: "dark" };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={appFont.variable}>
      <body className="min-h-screen max-w-[100vw] overflow-x-hidden bg-canvas font-sans text-ink antialiased">
        <QueryProvider>
          <PartyProvider>
            <MusicPlayerProvider>
              <TopBar />
              <main className="min-h-[70vh]">{children}</main>
              <Footer />
              <PlayerBar />
              <PartyFollower />
            </MusicPlayerProvider>
          </PartyProvider>
          <Toaster />
        </QueryProvider>
      </body>
    </html>
  );
}
