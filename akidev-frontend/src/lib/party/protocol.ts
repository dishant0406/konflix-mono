// The messages devices send each other in a watch party: JSON text on one data channel. The
// desktop app has the same list in desktop/party/src/protocol.rs, described in
// desktop/party/PROTOCOL.md. Change them together.

/** Something to play. Every device finds its own stream from the id, so no address travels. */
export type Item =
  | { kind: "video"; id: string }
  | { kind: "music"; id: string; title: string; artist: string; cover?: string | null };

/**
 * How a guest is doing with what the party is playing: it can play it now, it is still finding a
 * source or buffering (a film from a torrent can take a while), or it could not find or play it.
 */
export type Readiness = "ready" | "loading" | "trouble";

export interface Member {
  name: string;
  host: boolean;
  state: Readiness;
}

export type Message =
  | { t: "hello"; name: string }
  | { t: "state"; item: Item | null; playing: boolean; position: number; at: number }
  | { t: "members"; members: Member[] }
  | { t: "play" }
  | { t: "pause" }
  | { t: "seek"; position: number }
  | { t: "status"; state: Readiness; id: string | null }
  | { t: "ping"; id: number; sent: number }
  | { t: "pong"; id: number; sent: number; at: number }
  | { t: "bye" };

const isNumber = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
const isText = (value: unknown): value is string => typeof value === "string";
const isReadiness = (value: unknown): value is Readiness => value === "ready" || value === "loading" || value === "trouble";

/** The media id or the track id, whichever this item is. */
export function itemId(item: Item | null): string | null {
  return item ? item.id : null;
}

function parseItem(value: unknown): Item | null | undefined {
  if (value === null) return null;
  if (typeof value !== "object") return undefined;
  const item = value as Record<string, unknown>;
  if (item.kind === "video" && isText(item.id)) return { kind: "video", id: item.id };
  if (item.kind === "music" && isText(item.id) && isText(item.title) && isText(item.artist)) {
    return { kind: "music", id: item.id, title: item.title, artist: item.artist, cover: isText(item.cover) ? item.cover : null };
  }
  return undefined;
}

/** A message from the other device, or null when it is not one this app understands. */
export function parseMessage(text: string): Message | null {
  let raw: Record<string, unknown>;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null) return null;
    raw = value as Record<string, unknown>;
  } catch {
    return null;
  }
  switch (raw.t) {
    case "hello":
      return isText(raw.name) ? { t: "hello", name: raw.name } : null;
    case "state": {
      const item = parseItem(raw.item);
      if (item === undefined || typeof raw.playing !== "boolean" || !isNumber(raw.position) || !isNumber(raw.at)) return null;
      return { t: "state", item, playing: raw.playing, position: raw.position, at: raw.at };
    }
    case "members": {
      if (!Array.isArray(raw.members)) return null;
      const members: Member[] = [];
      for (const entry of raw.members as Record<string, unknown>[]) {
        if (!isText(entry?.name) || typeof entry.host !== "boolean" || !isReadiness(entry.state)) return null;
        members.push({ name: entry.name, host: entry.host, state: entry.state });
      }
      return { t: "members", members };
    }
    case "play":
    case "pause":
    case "bye":
      return { t: raw.t };
    case "seek":
      return isNumber(raw.position) ? { t: "seek", position: raw.position } : null;
    case "status":
      return isReadiness(raw.state) && (raw.id === null || isText(raw.id)) ? { t: "status", state: raw.state, id: raw.id } : null;
    case "ping":
      return isNumber(raw.id) && isNumber(raw.sent) ? { t: "ping", id: raw.id, sent: raw.sent } : null;
    case "pong":
      return isNumber(raw.id) && isNumber(raw.sent) && isNumber(raw.at) ? { t: "pong", id: raw.id, sent: raw.sent, at: raw.at } : null;
    default:
      return null;
  }
}

export function sameItem(a: Item | null, b: Item | null): boolean {
  return a === b || (a !== null && b !== null && a.kind === b.kind && a.id === b.id);
}
