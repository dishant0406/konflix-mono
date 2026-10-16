import type { Metadata } from "next";

import { Page, Section } from "@/components/prose";
import { REPO_URL } from "@/constants/site";

export const metadata: Metadata = {
  title: "Privacy policy | Akidev",
  description: "What Akidev, the site and the desktop app, does and does not do with your data.",
};

const link = "text-ink underline decoration-ink/30 underline-offset-4 transition-colors hover:decoration-ink";

export default function PrivacyPage() {
  return (
    <Page
      title="Privacy."
      lead="No accounts, nothing sold or shared. The site and the app count how they are used without knowing who you are, and streaming still needs some of your connection. This is all of it."
    >
      <Section title="What we collect">
        <p>
          Nothing that identifies you. There is no sign-up, no advertising and no cookie set by Akidev. Where you stopped in a title, your subtitle language and your settings are kept on your own device
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

      <Section title="Analytics and error reports">
        <p>
          To learn what is used and what breaks, the site uses PostHog (US cloud). It records the pages you open, when
          you press play on a title and whether playback worked, which download you clicked, and the errors the site
          runs into. Each visitor is a random identifier kept in your browser&apos;s local storage; it is not linked to a
          name, an email or an account, there are no session recordings, and nothing is sold or used for advertising.
          Events go through <code>akidev.live</code> to PostHog, so your browser contacts only Akidev. Clear the
          site&apos;s data to reset the identifier, or turn on Do Not Track in your browser, which the site honours.
        </p>
      </Section>

      <Section title="The desktop app">
        <p>
          The app makes the requests that its features need, and only those: it asks the catalogue providers (AniList,
          Cinemeta, TVMaze, YTS) for titles and artwork, searches public torrent indexers and subtitle providers
          (OpenSubtitles through its public addon) for what you chose to watch, and checks{" "}
          <code>sources.akidev.live</code> for updated source definitions. Apart from that it sends nothing about you.
        </p>
        <p>
          The app also reports how it is used and what breaks, to PostHog (US cloud). That is: when it opens and closes
          and for how long, which screens you open, which title you open or play and how long it took to start and how
          long it played, whether finding a source and playback worked, which subtitle language you chose, update
          checks and installs, and the errors and crashes it runs into (the message and where in the code it happened).
          Each report carries the app version and your operating system. An install is a random identifier made at the
          first start and kept in the app&apos;s settings file; it is not linked to a name, an email or an account.
          Your address is not kept, and the words you search for are never sent, only how long they were. This cannot be
          turned off in the app; deleting <code>config.toml</code> in the app&apos;s folder gives the install a new
          identifier, and you can block <code>us.i.posthog.com</code> to stop it entirely.
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
