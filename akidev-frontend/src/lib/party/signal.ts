// Talking to the meeting point on the API (/v1/party, see desktop/server/API.md).

import { API_BASE } from "../api.ts";

/** The party is over or never was, or the host's secret is not accepted: asking again is no use. */
export class Gone extends Error {}

export interface IceServer {
  urls: string[];
  username?: string;
  credential?: string;
}

/** A signal that fires after `ms`, or when `also` does (`AbortSignal.any` is not in every browser). */
function stopAfter(ms: number, also?: AbortSignal): AbortSignal {
  const stop = new AbortController();
  const timer = setTimeout(() => stop.abort(), ms);
  const finish = () => {
    clearTimeout(timer);
    stop.abort();
  };
  if (also?.aborted) finish();
  also?.addEventListener("abort", finish, { once: true });
  return stop.signal;
}

async function call(path: string, options: { method?: string; token?: string; body?: unknown; timeout?: number; signal?: AbortSignal; keepalive?: boolean } = {}): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(`${API_BASE}/v1/party${path}`, {
      method: options.method ?? "GET",
      headers: {
        ...(options.body === undefined ? {} : { "Content-Type": "application/json" }),
        ...(options.token ? { Authorization: `Bearer ${options.token}` } : {}),
      },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      keepalive: options.keepalive,
      signal: stopAfter(options.timeout ?? 15_000, options.signal),
    });
  } catch {
    throw new Error("Could not reach the party server.");
  }
  if (response.ok) return response;
  const body: unknown = await response.json().catch(() => null);
  const message =
    body && typeof body === "object" && "error" in body && typeof (body as { error: unknown }).error === "string"
      ? (body as { error: string }).error
      : `The party server answered ${response.status}.`;
  throw response.status === 404 || response.status === 401 ? new Gone(message) : new Error(message);
}

export async function create(): Promise<{ code: string; token: string }> {
  const body = (await (await call("", { method: "POST" })).json()) as { code: string; host_token: string };
  return { code: body.code, token: body.host_token };
}

export async function ice(code: string): Promise<IceServer[]> {
  return ((await (await call(`/${code}/ice`)).json()) as { ice_servers: IceServer[] }).ice_servers;
}

/** Posts a guest's offer and returns the guest's id. */
export async function join(code: string, offer: string): Promise<string> {
  const response = await call(`/${code}/join`, { method: "POST", body: { offer } });
  return ((await response.json()) as { guest: string }).guest;
}

/** The offers waiting for the host. Empty when none came within the wait. */
export async function offers(code: string, token: string, signal: AbortSignal): Promise<{ guest: string; offer: string }[]> {
  const response = await call(`/${code}/offers`, { token, timeout: 40_000, signal });
  return ((await response.json()) as { offers: { guest: string; offer: string }[] }).offers;
}

export async function answer(code: string, token: string, guest: string, sdp: string): Promise<void> {
  await call(`/${code}/answer/${guest}`, { method: "POST", token, body: { answer: sdp } });
}

/** The host's answer, or null when it has not come within the wait. */
export async function takeAnswer(code: string, guest: string): Promise<string | null> {
  const response = await call(`/${code}/answer/${guest}`, { timeout: 40_000 });
  if (response.status === 204) return null;
  return ((await response.json()) as { answer: string }).answer;
}

/** Best effort: a guest has gone, so its place can be given to another. */
export function guestGone(code: string, token: string, guest: string) {
  void call(`/${code}/guests/${guest}`, { method: "DELETE", token }).catch(() => {});
}

/** Best effort: the party is over. `keepalive` lets it go out as the page closes. */
export function end(code: string, token: string) {
  void call(`/${code}`, { method: "DELETE", token, keepalive: true }).catch(() => {});
}
