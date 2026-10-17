"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";

import { joinParty, openParty, type GuestParty, type HostParty, type Request } from "@/lib/party/connection";
import { sameItem, type Item, type Member, type Readiness } from "@/lib/party/protocol";
import type { Playback, Target } from "@/lib/party/sync";

type Role = "none" | "host" | "guest";

interface Party {
  /** Hosting a party, following someone else's, or neither. */
  role: Role;
  connecting: boolean;
  code: string | null;
  members: Member[];
  /** Why the last attempt to start or join failed, or the party stopped working. */
  error: string | null;
  /** What the party is playing: what the host last published, or what the host last said. */
  item: Item | null;
  /** A guest's latest word from the host. */
  target: Target | null;
  /** The code in the address the page was opened with, for the panel to offer to join. */
  invite: string | null;
  name: string;
  setName: (name: string) => void;
  create: () => Promise<void>;
  join: (code: string) => Promise<void>;
  leave: () => void;
  /** The host says what is playing: when it changes, and every couple of seconds while it plays.
   *  A `heartbeat` from something that is not what is playing is ignored. */
  publish: (playback: Playback, heartbeat?: boolean) => void;
  /** A guest asks the host to do something. */
  ask: (request: Request) => void;
  /** A guest says how it is doing with the item whose id is `id`: ready, still loading, or unable. */
  setStatus: (state: Readiness, id: string | null) => void;
  /** The names of the guests who are still loading what the host is playing, for the host. */
  waitingFor: string[];
  /** The host is told what guests ask for. Returns what stops it. */
  onRequest: (handler: (request: Request) => void) => () => void;
}

const Context = createContext<Party | null>(null);

export function useParty(): Party {
  const party = useContext(Context);
  if (!party) throw new Error("useParty needs PartyProvider");
  return party;
}

const NAME_KEY = "akidev:party-name";

function rememberedName(): string {
  try {
    const saved = localStorage.getItem(NAME_KEY);
    if (saved) return saved;
  } catch {
    // Storage is blocked: a name for this visit will do.
  }
  return `Guest ${Math.floor(1000 + Math.random() * 9000)}`;
}

export default function PartyProvider({ children }: { children: React.ReactNode }) {
  const [role, setRole] = useState<Role>("none");
  const [connecting, setConnecting] = useState(false);
  const [code, setCode] = useState<string | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [item, setItem] = useState<Item | null>(null);
  const [target, setTarget] = useState<Target | null>(null);
  const [invite, setInvite] = useState<string | null>(null);
  const [name, setNameState] = useState("Guest");

  const host = useRef<HostParty | null>(null);
  const guest = useRef<GuestParty | null>(null);
  const handlers = useRef(new Set<(request: Request) => void>());
  const published = useRef<Item | null>(null);
  // What this guest last told the host, so it only tells it when that changes.
  const said = useRef("");

  useEffect(() => {
    setNameState(rememberedName());
    const code = new URLSearchParams(window.location.search).get("party");
    if (code) setInvite(code.trim().toUpperCase().slice(0, 12));
  }, []);

  const setName = useCallback((next: string) => {
    setNameState(next);
    try {
      localStorage.setItem(NAME_KEY, next);
    } catch {
      // Not remembered, which only means asking again next time.
    }
  }, []);

  const reset = useCallback(() => {
    host.current = null;
    guest.current = null;
    published.current = null;
    said.current = "";
    setRole("none");
    setConnecting(false);
    setCode(null);
    setMembers([]);
    setItem(null);
    setTarget(null);
  }, []);

  const leave = useCallback(() => {
    host.current?.end();
    guest.current?.leave();
    reset();
  }, [reset]);

  // A host closing the page ends the party, so it is not left open on the server. A page that
  // is only being kept for the back button comes back with the party already over.
  useEffect(() => {
    const goodbye = (event: PageTransitionEvent) => {
      host.current?.end();
      if (event.persisted) reset();
    };
    window.addEventListener("pagehide", goodbye);
    return () => window.removeEventListener("pagehide", goodbye);
  }, [reset]);

  const create = useCallback(async () => {
    leave();
    setError(null);
    setConnecting(true);
    try {
      const party = await openParty(name, {
        members: setMembers,
        request: (request) => handlers.current.forEach((handler) => handler(request)),
        lost: () => setError("Lost contact with the party server, so no one new can join."),
      });
      host.current = party;
      setRole("host");
      setCode(party.code);
      setMembers([{ name, host: true, state: "ready" }]);
      setInvite(null);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Could not start the party.");
    }
    setConnecting(false);
  }, [leave, name]);

  const join = useCallback(
    async (wanted: string) => {
      leave();
      setError(null);
      setConnecting(true);
      try {
        const party = await joinParty(wanted, name, {
          target: (next) => {
            setTarget(next);
            setItem((current) => (sameItem(current, next.playback.item) ? current : next.playback.item));
          },
          members: setMembers,
          ended: () => {
            reset();
            setError("The party ended.");
          },
        });
        guest.current = party;
        setRole("guest");
        setCode(wanted.trim().toUpperCase());
        setInvite(null);
      } catch (failure) {
        setError(failure instanceof Error ? failure.message : "Could not join the party.");
      }
      setConnecting(false);
    },
    [leave, name, reset],
  );

  const publish = useCallback((playback: Playback, heartbeat = false) => {
    if (!host.current) return;
    if (heartbeat && !sameItem(playback.item, published.current)) return;
    published.current = playback.item;
    setItem((current) => (sameItem(current, playback.item) ? current : playback.item));
    host.current.publish(playback);
  }, []);

  const ask = useCallback((request: Request) => guest.current?.ask(request), []);

  const setStatus = useCallback((state: Readiness, id: string | null) => {
    const now = `${state}|${id}`;
    if (said.current === now) return;
    said.current = now;
    guest.current?.status(state, id);
  }, []);

  const waitingFor = useMemo(
    () => members.filter((member) => !member.host && member.state === "loading").map((member) => member.name),
    [members],
  );

  const onRequest = useCallback((handler: (request: Request) => void) => {
    handlers.current.add(handler);
    return () => void handlers.current.delete(handler);
  }, []);

  const value = useMemo<Party>(
    () => ({
      role,
      connecting,
      code,
      members,
      error,
      item,
      target,
      invite,
      name,
      setName,
      create,
      join,
      leave,
      publish,
      ask,
      setStatus,
      waitingFor,
      onRequest,
    }),
    [role, connecting, code, members, error, item, target, invite, name, setName, create, join, leave, publish, ask, setStatus, waitingFor, onRequest],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}
