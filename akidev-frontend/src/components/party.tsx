"use client";

import { Check, ClipboardPaste, Link2, Loader2, Users } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import { useParty } from "@/providers/party";
import type { Member } from "@/lib/party/protocol";

import { IconButton } from "./button";

/** The code's letters and digits, as the server makes them: the ones that are not easily mistaken. */
const CODE_LENGTH = 6;
const clean = (text: string) => text.toUpperCase().replace(/[^A-HJ-NP-Z2-9]/g, "").slice(0, CODE_LENGTH);

/** The party button for a bar, and the panel it opens. */
export default function PartyButton({ className }: { className?: string }) {
  const party = useParty();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const live = party.role !== "none";

  // A link to a party opens the panel with its code ready.
  useEffect(() => {
    if (party.invite) setOpen(true);
  }, [party.invite]);

  // A press anywhere else, or Escape, puts it away.
  useEffect(() => {
    if (!open) return undefined;
    const away = (event: MouseEvent) => !box.current?.contains(event.target as Node) && setOpen(false);
    const escape = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    window.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", escape);
    };
  }, [open]);

  return (
    <div ref={box} className="relative">
      <IconButton
        label={live ? `Party, ${party.members.length} here` : "Watch party"}
        onClick={() => setOpen((was) => !was)}
        className={cn((open || live) && "bg-wash-2", className)}
      >
        <span className="relative">
          <Users className="size-[18px]" />
          {live && <span className="absolute -right-1 -top-0.5 size-2 rounded-full bg-accent ring-2 ring-canvas" />}
        </span>
      </IconButton>
      {open && (
        <div
          role="dialog"
          aria-label="Watch party"
          className="absolute right-0 top-full z-[70] mt-2 w-[320px] max-w-[calc(100vw-2rem)] origin-top-right animate-pop material-panel rounded-xl p-4 shadow-[0_0_0_1px_rgb(var(--ink)/0.08),0_24px_60px_rgb(0_0_0/0.55)]"
        >
          <Panel />
        </div>
      )}
    </div>
  );
}

function Panel() {
  const party = useParty();
  return (
    <div className="flex flex-col gap-4">
      {party.connecting ? (
        <Working />
      ) : party.role === "none" ? (
        <Idle />
      ) : (
        <Live />
      )}
      {party.error && !party.connecting && (
        <p className="flex items-start gap-2 text-[13px] leading-snug text-accent">
          <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-accent" />
          {party.error}
        </p>
      )}
    </div>
  );
}

function Working() {
  return (
    <div className="flex items-center justify-center gap-2.5 py-8 text-sm text-dim">
      <Loader2 className="size-4 animate-spin" />
      Connecting
    </div>
  );
}

/** Start a party, or join one. */
function Idle() {
  const party = useParty();
  const [tab, setTab] = useState<"start" | "join">(party.invite ? "join" : "start");
  return (
    <>
      <div role="tablist" aria-label="Start or join" className="grid grid-cols-2 gap-0.5 rounded-[10px] bg-wash p-0.5">
        {(["start", "join"] as const).map((which) => (
          <button
            key={which}
            type="button"
            role="tab"
            aria-selected={tab === which}
            onClick={() => setTab(which)}
            className={cn(
              "h-8 cursor-pointer rounded-lg text-[13px] font-semibold transition-colors",
              tab === which ? "bg-wash-2 text-ink" : "text-dim hover:text-ink",
            )}
          >
            {which === "start" ? "Start" : "Join"}
          </button>
        ))}
      </div>
      {tab === "start" ? <Start /> : <Join />}
    </>
  );
}

function Start() {
  const party = useParty();
  return (
    <>
      <label className="flex items-center gap-3 rounded-xl bg-wash px-3 py-2.5 transition-colors focus-within:bg-wash">
        <Avatar name={party.name} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="text-[11px] font-medium text-faint">You appear as</span>
          <input
            value={party.name}
            onChange={(event) => party.setName(event.target.value)}
            maxLength={24}
            spellCheck={false}
            className="min-w-0 bg-transparent text-[15px] font-medium text-ink outline-none"
          />
        </span>
      </label>
      <p className="text-[13px] leading-snug text-dim">Everyone plays what you play, in step. Share the code, and they join in seconds.</p>
      <Action onClick={() => void party.create()}>Start party</Action>
    </>
  );
}

function Join() {
  const party = useParty();
  const [typed, setTyped] = useState(clean(party.invite ?? ""));
  const [focused, setFocused] = useState(false);

  const change = (text: string) => {
    const next = clean(text);
    setTyped(next);
    // The last letter joins, so there is nothing more to press.
    if (next.length === CODE_LENGTH && next !== typed) void party.join(next);
  };
  const paste = async () => {
    try {
      change(await navigator.clipboard.readText());
    } catch {
      toast.error("Could not read the clipboard. Paste into the boxes instead.");
    }
  };

  return (
    <>
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <label htmlFor="party-code" className="text-[11px] font-medium text-faint">
            Party code
          </label>
          <button type="button" onClick={() => void paste()} className="flex cursor-pointer items-center gap-1 text-[12px] font-medium text-dim transition-colors hover:text-ink">
            <ClipboardPaste className="size-3.5" />
            Paste
          </button>
        </div>
        <div className="relative">
          <div className="grid grid-cols-6 gap-1.5" aria-hidden>
            {Array.from({ length: CODE_LENGTH }, (_, index) => (
              <span
                key={index}
                className={cn(
                  "flex h-12 items-center justify-center rounded-lg bg-wash font-mono text-xl font-semibold text-ink ring-1 ring-inset transition-shadow",
                  focused && index === Math.min(typed.length, CODE_LENGTH - 1) ? "ring-accent" : "ring-line",
                )}
              >
                {typed[index] ?? ""}
              </span>
            ))}
          </div>
          {/* One real field over the boxes, so typing, pasting and autofill all work. */}
          <input
            id="party-code"
            value={typed}
            onChange={(event) => change(event.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            maxLength={CODE_LENGTH + 12}
            autoCapitalize="characters"
            autoComplete="off"
            spellCheck={false}
            aria-label="Party code"
            className="absolute inset-0 size-full cursor-text opacity-0"
          />
        </div>
      </div>
      <p className="text-[13px] leading-snug text-dim">
        {party.invite ? "You were invited to this party. Press join to come in." : "Ask the host for their six letter code. You join as soon as it is complete."}
      </p>
      <Action onClick={() => void party.join(typed)} disabled={typed.length < CODE_LENGTH} quiet>
        Join party
      </Action>
    </>
  );
}

/** In a party: the code to share, who is here, and a way out. */
function Live() {
  const party = useParty();
  const [copied, setCopied] = useState<"code" | "link" | null>(null);
  const code = party.code ?? "";
  const host = party.role === "host";

  const copy = async (what: "code" | "link") => {
    try {
      await navigator.clipboard.writeText(what === "code" ? code : `${window.location.origin}/?party=${code}`);
      setCopied(what);
      window.setTimeout(() => setCopied(null), 1600);
    } catch {
      toast.error("Could not copy. Select the code instead.");
    }
  };

  return (
    <>
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-medium text-faint">{host ? "Party code" : "You are in party"}</span>
          <button type="button" onClick={() => void copy("link")} className="flex cursor-pointer items-center gap-1 text-[12px] font-medium text-dim transition-colors hover:text-ink">
            {copied === "link" ? <Check className="size-3.5 text-accent" /> : <Link2 className="size-3.5" />}
            {copied === "link" ? "Link copied" : "Copy link"}
          </button>
        </div>
        <button
          type="button"
          onClick={() => void copy("code")}
          aria-label={`Copy code ${code}`}
          className="group relative grid cursor-pointer grid-cols-6 gap-1.5 text-left"
        >
          {Array.from({ length: CODE_LENGTH }, (_, index) => (
            <span
              key={index}
              className="flex h-12 items-center justify-center rounded-lg bg-wash font-mono text-xl font-semibold text-ink ring-1 ring-inset ring-line transition-colors group-hover:bg-wash"
            >
              {code[index]}
            </span>
          ))}
          <span
            className={cn(
              "pointer-events-none absolute inset-0 flex items-center justify-center gap-1.5 rounded-lg bg-surface/90 text-[13px] font-semibold text-ink opacity-0 transition-opacity",
              copied === "code" && "opacity-100",
            )}
          >
            <Check className="size-4 text-accent" />
            Copied
          </span>
        </button>
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] font-medium text-faint">In this party · {party.members.length}</span>
        <ul aria-label="In this party" className="-mx-1 flex max-h-56 flex-col overflow-y-auto">
          {party.members.map((member, index) => (
            <Person key={`${member.name}-${index}`} member={member} />
          ))}
        </ul>
      </div>

      <p className="text-[12px] leading-snug text-faint">
        {host ? "Anyone can pause and seek. You choose what plays." : "The host chooses what plays. You can pause and seek for everyone."}
      </p>
      <button
        type="button"
        onClick={party.leave}
        className="h-9 cursor-pointer rounded-md border border-ink/15 text-[13px] font-medium text-dim transition-[colors,transform] duration-150 ease-spring active:scale-[0.97] hover:border-danger/40 hover:bg-danger/10 hover:text-danger"
      >
        {host ? "End party" : "Leave party"}
      </button>
    </>
  );
}

function Person({ member }: { member: Member }) {
  return (
    <li className="flex items-center gap-3 rounded-lg px-1 py-1.5">
      <Avatar name={member.name} host={member.host} />
      <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{member.name}</span>
      <Status member={member} />
    </li>
  );
}

/** Quiet while all is well; the host and anyone having trouble are the ones to notice. */
function Status({ member }: { member: Member }) {
  if (member.state === "trouble") return <span className="text-xs font-medium text-danger">Can&apos;t play</span>;
  if (member.state === "loading") {
    return (
      <span className="flex items-center gap-1.5 text-xs font-medium text-accent">
        <span className="size-1.5 animate-pulse rounded-full bg-accent" />
        Loading
      </span>
    );
  }
  return member.host ? <span className="text-xs font-medium text-faint">Host</span> : <span className="size-1.5 rounded-full bg-ink/30" aria-label="Ready" />;
}

/** The first letter of a name, in a circle. The host's has a ring. */
function Avatar({ name, host }: { name: string; host?: boolean }) {
  return (
    <span
      className={cn(
        "flex size-8 shrink-0 items-center justify-center rounded-full bg-wash text-[13px] font-semibold uppercase text-ink",
        host && "ring-1 ring-accent/70",
      )}
      aria-hidden
    >
      {(name.trim()[0] ?? "?").toUpperCase()}
    </span>
  );
}

function Action({ onClick, disabled, quiet, children }: { onClick: () => void; disabled?: boolean; quiet?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "h-11 cursor-pointer rounded-xl text-[15px] font-semibold transition-colors disabled:pointer-events-none disabled:opacity-40",
        quiet ? "bg-wash-2 text-ink hover:bg-wash-3" : "bg-ink text-canvas hover:bg-ink/85 active:bg-ink/75",
      )}
    >
      {children}
    </button>
  );
}
