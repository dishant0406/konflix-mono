import type { Metadata } from "next";

import { Page, Section } from "@/components/prose";
import { REPO_URL } from "@/constants/site";

export const metadata: Metadata = {
  title: "Privacy policy | Akidev",
  description: "What Akidev, the site and the desktop app, does and does not do with your data.",
};

const link = "text-ink underline underline-offset-2 hover:text-accent";

export default function PrivacyPage() {
  return (
    <Page
      title="Privacy policy"
      lead="Akidev has no accounts and no analytics, and it does not sell or share personal data. This page says what still goes over the network, because streaming needs some of it."
    >
      <Section title="What we collect">
        <p>
          Nothing that identifies you. There is no sign-up, no tracking script, no advertising and no cookie set by
          Akidev. Where you stopped in a title, your subtitle language and your settings are kept on your own device
          (in the browser, or in the desktop app&apos;s folder) and are never sent to us.
        </p>
      </Section>

      <Section title="The site">
        <p>
          The site is served through Cloudflare and talks to the Akidev API. Like any web server, they see the address
          of your connection and the pages you request, and keep short-lived technical logs to run and protect the
          service. Searching or playing sends the title or search term to the API so it can answer.
        </p>
      </Section>

      <Section title="The desktop app">
        <p>
          The app makes the requests that its features need, and only those: it asks the catalogue providers (AniList,
          Cinemeta, TVMaze, YTS) for titles and artwork, searches public torrent indexers and subtitle providers
          (OpenSubtitles through its public addon) for what you chose to watch, and checks{" "}
          <code>sources.akidev.live</code> for updated source definitions. It sends no usage statistics or crash
          reports.
        </p>
        <p>
          Playing a title joins a BitTorrent swarm. As with any BitTorrent client, the other peers in that swarm can see
          your IP address. Use a VPN if that matters to you. What was fetched is deleted when you quit.
        </p>
      </Section>

      <Section title="Contact">
        <p>
          Akidev is open source. Questions or concerns can be raised as an issue at{" "}
          <a href={REPO_URL} className={link} rel="noopener">
            the repository
          </a>
          .
        </p>
      </Section>
    </Page>
  );
}
