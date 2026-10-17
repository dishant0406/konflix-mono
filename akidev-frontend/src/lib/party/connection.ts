// The two ends of a party over WebRTC: the host that holds it and the guest that follows. The
// desktop app does the same in desktop/party/src/{host,guest}.rs.

import { itemId, parseMessage, type Member, type Message, type Readiness } from "./protocol.ts";
import * as signal from "./signal.ts";
import { Clock, targetOf, type Playback, type Target } from "./sync.ts";

/** How long finding the way to the other device (gathering addresses, asking STUN) may take. */
const GATHER_FOR = 10_000;
/** How long the other device has to open the channel once the answer is in. */
const OPEN_WITHIN = 30_000;
/** How long a guest keeps asking for the host's answer. */
const ANSWER_WITHIN = 60_000;
/** How often the clocks are compared. The first few are quick, so a good reading comes early. */
const PING_EVERY = 5_000;
const QUICK_PINGS = 5;
/** How long the host's connections stay open after it says goodbye, for the goodbye to arrive. */
const CLOSE_AFTER = 500;
/** Failing to reach the party server this many times in a row ends the wait for guests. */
const GIVE_UP_AFTER = 6;

function send(channel: RTCDataChannel, message: Message) {
  if (channel.readyState === "open") channel.send(JSON.stringify(message));
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Resolves once every address has been found, so the description can be sent whole. */
function gathered(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const finish = () => {
      pc.removeEventListener("icegatheringstatechange", check);
      clearTimeout(timer);
      resolve();
    };
    const check = () => pc.iceGatheringState === "complete" && finish();
    // Gathering that has not finished by now is sent as it is: it has the addresses found.
    const timer = setTimeout(finish, GATHER_FOR);
    pc.addEventListener("icegatheringstatechange", check);
  });
}

/** A published state moved on to now: a playing one has been playing since it was said. */
function asOfNow(state: Message): Message {
  if (state.t !== "state" || !state.playing) return state;
  const now = Date.now();
  return { ...state, position: state.position + Math.max(0, now - state.at) / 1000, at: now };
}

/** A name from a guest is shown to everyone, so it is kept short and plain. */
export function cleanName(name: string): string {
  const clean = name.replace(/[\u0000-\u001f\u007f]/g, "").slice(0, 24).trim();
  return clean || "Guest";
}

export type Request = Extract<Message, { t: "play" | "pause" | "seek" }>;

export interface HostEvents {
  /** Someone joined or left, or how someone is doing with what is playing changed. */
  members: (members: Member[]) => void;
  /** A guest asked for something. The host does it, as if a person at the host had. */
  request: (request: Request) => void;
  /** The party server stopped answering for the host, so no one new can join. */
  lost: () => void;
}

export interface HostParty {
  code: string;
  /** What is playing now. Called when it changes and every couple of seconds while it plays. */
  publish: (playback: Playback) => void;
  /** Ends the party: guests are told, and the party is closed on the server. */
  end: () => void;
}

interface GuestSlot {
  channel: RTCDataChannel;
  name: string;
  /** What the guest last said about how it is doing, and about which item. */
  state: Readiness;
  about: string | null;
}

/**
 * How a guest is doing with what the host is playing now. What it said about an earlier item says
 * nothing about this one: it is still getting to it.
 */
export function guestState(said: Readiness, about: string | null, playing: string | null): Readiness {
  return playing !== null && about !== playing ? "loading" : said;
}

/** Opens a party. `name` is how the host appears to the guests. */
export async function openParty(name: string, on: HostEvents): Promise<HostParty> {
  const { code, token } = await signal.create();
  const stop = new AbortController();
  const guests = new Map<string, GuestSlot>();
  const connections = new Set<RTCPeerConnection>();
  /** What was last published, so a guest who joins late is told at once. */
  let last: Message | null = null;

  const broadcast = (message: Message) => guests.forEach((slot) => send(slot.channel, message));
  const shareMembers = () => {
    const playing = last?.t === "state" ? itemId(last.item) : null;
    const members: Member[] = [
      { name, host: true, state: "ready" },
      ...[...guests.values()].map((slot): Member => ({ name: slot.name, host: false, state: guestState(slot.state, slot.about, playing) })),
    ];
    broadcast({ t: "members", members });
    on.members(members);
  };

  /** Answers one guest's offer and looks after that guest until they are gone. */
  const accept = async (guest: string, offer: string) => {
    let pc: RTCPeerConnection | null = null;
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      if (pc) {
        connections.delete(pc);
        pc.close();
      }
      signal.guestGone(code, token, guest);
      if (guests.delete(guest)) shareMembers();
    };
    const connect = (iceServers: signal.IceServer[]): RTCPeerConnection => {
      const connection = new RTCPeerConnection({ iceServers });
      connections.add(connection);
      connection.onconnectionstatechange = () => {
        if (connection.connectionState === "failed" || connection.connectionState === "closed") finish();
      };
      connection.ondatachannel = ({ channel }) => listen(channel);
      return connection;
    };
    const listen = (channel: RTCDataChannel) => {
      channel.onopen = () => {
        guests.set(guest, { channel, name: "Guest", state: "ready", about: null });
        if (last) send(channel, asOfNow(last));
        shareMembers();
      };
      channel.onclose = finish;
      channel.onmessage = (event) => {
        const message = typeof event.data === "string" ? parseMessage(event.data) : null;
        const slot = guests.get(guest);
        if (!message || !slot) return;
        switch (message.t) {
          case "hello":
            slot.name = cleanName(message.name);
            shareMembers();
            break;
          case "status":
            slot.state = message.state;
            slot.about = message.id;
            shareMembers();
            break;
          case "ping":
            send(channel, { t: "pong", id: message.id, sent: message.sent, at: Date.now() });
            break;
          case "play":
          case "pause":
            on.request(message);
            break;
          case "seek":
            on.request({ t: "seek", position: Math.max(0, message.position) });
            break;
          case "bye":
            finish();
            break;
        }
      };
    };
    try {
      // Asked for each time: the relay's login lasts a few hours, and a party can last longer.
      pc = connect(await signal.ice(code));
      await pc.setRemoteDescription({ type: "offer", sdp: offer });
      await pc.setLocalDescription(await pc.createAnswer());
      await gathered(pc);
      await signal.answer(code, token, guest, pc.localDescription?.sdp ?? "");
      setTimeout(() => guests.has(guest) || finish(), OPEN_WITHIN);
    } catch {
      finish();
    }
  };

  /** Asks the server for guests, one after another, and connects each as it comes. */
  const listen = async () => {
    let failures = 0;
    while (!stop.signal.aborted) {
      try {
        const offers = await signal.offers(code, token, stop.signal);
        failures = 0;
        offers.forEach((entry) => void accept(entry.guest, entry.offer));
      } catch (error) {
        if (stop.signal.aborted) return;
        // The server no longer has the party (it was restarted, or the host was away too long).
        if (error instanceof signal.Gone) return on.lost();
        if (++failures >= GIVE_UP_AFTER) return on.lost();
        await sleep(2000);
      }
    }
  };
  void listen();

  return {
    code,
    publish: (playback) => {
      const before = last?.t === "state" ? itemId(last.item) : null;
      last = { t: "state", item: playback.item, playing: playback.playing, position: playback.position, at: Date.now() };
      broadcast(last);
      // A guest is loading whatever the host starts until it says it is ready for it, so a new
      // item changes who is.
      if (before !== itemId(playback.item)) shareMembers();
    },
    end: () => {
      stop.abort();
      broadcast({ t: "bye" });
      signal.end(code, token);
      guests.clear();
      // Closing at once can lose the goodbye, so the guests would only find out when their
      // connection timed out.
      setTimeout(() => connections.forEach((pc) => pc.close()), CLOSE_AFTER);
    },
  };
}

export interface GuestEvents {
  /** What the party should be playing now. Sent when the host changes it and every few seconds. */
  target: (target: Target) => void;
  members: (members: Member[]) => void;
  /** The host ended the party or the connection to it is gone. */
  ended: () => void;
}

export interface GuestParty {
  /** Asks the host to play, pause or seek. The host does it, and everyone, this device
   *  included, hears what it is now. */
  ask: (request: Request) => void;
  /** Tells the host how this device is doing with the item whose id is `id`. */
  status: (state: Readiness, id: string | null) => void;
  leave: () => void;
}

/** Joins the party `code`. `name` is how this device appears to the others. */
export async function joinParty(code: string, name: string, on: GuestEvents): Promise<GuestParty> {
  code = code.trim().toUpperCase();
  const ice = await signal.ice(code);
  const pc = new RTCPeerConnection({ iceServers: ice });
  const channel = pc.createDataChannel("party");
  const clock = new Clock();
  let over = false;
  const timers: ReturnType<typeof setInterval>[] = [];
  /** Lets go of the party. The page is told, unless it was this device that left. */
  const end = (notify: boolean) => {
    if (over) return;
    over = true;
    timers.forEach(clearInterval);
    pc.close();
    if (notify) on.ended();
  };

  try {
    await pc.setLocalDescription(await pc.createOffer());
    await gathered(pc);
    const guest = await signal.join(code, pc.localDescription?.sdp ?? "");
    let answer: string | null = null;
    for (const started = Date.now(); !answer; ) {
      if (Date.now() - started > ANSWER_WITHIN) throw new Error("The host did not answer.");
      answer = await signal.takeAnswer(code, guest);
    }
    await pc.setRemoteDescription({ type: "answer", sdp: answer });
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("The host did not connect.")), OPEN_WITHIN);
      channel.onopen = () => {
        clearTimeout(timer);
        resolve();
      };
      channel.onclose = () => reject(new Error("The connection closed."));
    });
  } catch (error) {
    over = true;
    pc.close();
    throw error;
  }

  channel.onclose = () => end(true);
  pc.onconnectionstatechange = () => {
    if (pc.connectionState === "failed" || pc.connectionState === "closed") end(true);
  };
  // What the host said before the clocks were known is kept, and acted on at the first reading:
  // without one, how long ago it was said cannot be told.
  let waiting: Extract<Message, { t: "state" }> | null = null;
  channel.onmessage = (event) => {
    const message = typeof event.data === "string" ? parseMessage(event.data) : null;
    if (!message) return;
    if (message.t === "state") {
      if (clock.known) on.target(targetOf(message, clock, Date.now()));
      else waiting = message;
    } else if (message.t === "members") on.members(message.members);
    else if (message.t === "pong") {
      clock.sample(message.sent, message.at, Date.now());
      if (waiting) on.target(targetOf(waiting, clock, Date.now()));
      waiting = null;
    } else if (message.t === "bye") end(true);
  };
  send(channel, { t: "hello", name: cleanName(name) });
  let pings = 0;
  const ping = () => send(channel, { t: "ping", id: pings++, sent: Date.now() });
  ping();
  // The first few are quick, so a good reading comes early.
  const quick = setInterval(() => {
    ping();
    if (pings >= QUICK_PINGS) clearInterval(quick);
  }, 400);
  timers.push(quick, setInterval(ping, PING_EVERY));

  return {
    ask: (request) => send(channel, request),
    status: (state, id) => send(channel, { t: "status", state, id }),
    leave: () => {
      send(channel, { t: "bye" });
      end(false);
    },
  };
}
