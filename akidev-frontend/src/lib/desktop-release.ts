import { REPO_URL } from "../constants/site.ts";

export type Platform = "windows" | "mac";

export interface Download {
  version: string;
  /** The file itself when the release has one, else the release page. */
  url: string;
}

interface ApiRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  html_url: string;
  assets: { name: string; browser_download_url: string }[];
}

const TAG: Record<Platform, string> = { windows: "desktop-windows-v", mac: "desktop-mac-v" };
const FILE: Record<Platform, RegExp> = { windows: /-setup\.exe$/, mac: /\.dmg$/ };

/** The newest release of a platform in a list the API returned, newest first. */
export function pick(releases: ApiRelease[], platform: Platform): Download | null {
  const release = releases.find((each) => !each.draft && !each.prerelease && each.tag_name.startsWith(TAG[platform]));
  if (!release) return null;
  const asset = release.assets.find((each) => FILE[platform].test(each.name));
  return { version: release.tag_name.slice(TAG[platform].length), url: asset?.browser_download_url ?? release.html_url };
}

/**
 * The two platforms release from their own tags, so "latest" on GitHub would be only one of them.
 * Read once an hour with the page; if GitHub does not answer, the page links to the releases list.
 */
export async function latestDownloads(): Promise<Record<Platform, Download | null>> {
  try {
    const response = await fetch(`${REPO_URL.replace("github.com", "api.github.com/repos")}/releases?per_page=30`, {
      next: { revalidate: 3600 },
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) throw new Error(String(response.status));
    const releases = (await response.json()) as ApiRelease[];
    return { windows: pick(releases, "windows"), mac: pick(releases, "mac") };
  } catch {
    return { windows: null, mac: null };
  }
}
