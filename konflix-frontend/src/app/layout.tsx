import type { Metadata } from "next";
import "./globals.css";

import Footer from "@/components/footer";
import Navbar from "@/components/navbar";
import { Toaster } from "@/components/ui/sonner";
import { appFont } from "@/lib/fonts";
import QueryProvider from "@/providers/query-provider";

const APP_NAME = "Konflix";
const APP_TITLE = "Konflix | Anime Streaming";
const APP_DESCRIPTION = "Browse and stream anime, with metadata from AniList.";

export const metadata: Metadata = {
  applicationName: APP_NAME,
  title: APP_TITLE,
  description: APP_DESCRIPTION,
  icons: { icon: "/favicon.ico" },
  openGraph: {
    type: "website",
    siteName: APP_NAME,
    title: APP_TITLE,
    description: APP_DESCRIPTION,
  },
  twitter: {
    card: "summary",
    title: APP_TITLE,
    description: APP_DESCRIPTION,
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className={`${appFont.className} min-h-screen max-w-[100vw] overflow-x-hidden bg-background antialiased`}>
        <QueryProvider>
          <Navbar />
          <main className="min-h-[70vh] pt-[4.25rem]">{children}</main>
          <Footer />
          <Toaster />
        </QueryProvider>
      </body>
    </html>
  );
}
