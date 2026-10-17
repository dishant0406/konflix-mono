// The single REST client used by the whole app.
//
// The API is JSON over HTTP (desktop/server/API.md), so this is a small fetch wrapper: it
// also keeps the request count and the bundle honest.

export const API_BASE = (process.env.NEXT_PUBLIC_API_URL || "http://localhost:9000").replace(/\/+$/, "");

// Finding a source searches torrent indexers and can legitimately take a while, but it must
// never hang. The server bounds its own work to about a minute, so this sits above that and
// turns a stuck request into a visible error instead of a permanent spinner.
const TIMEOUT_MS = Number(process.env.NEXT_PUBLIC_API_TIMEOUT_MS) || 90000;

/** A failed response: the server's `{ error }` message and the HTTP status. */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export type Params = Record<string, string | number | boolean | null | undefined>;

export async function request<T>(
  path: string,
  options: { method?: "GET" | "POST"; params?: Params; body?: unknown } = {},
): Promise<T> {
  const query = new URLSearchParams();
  for (const [name, value] of Object.entries(options.params ?? {})) {
    if (value !== undefined && value !== null && value !== "") query.set(name, String(value));
  }
  const url = `${API_BASE}${path}${query.size > 0 ? `?${query}` : ""}`;

  let response: Response;
  try {
    response = await fetch(url, {
      method: options.method ?? "GET",
      headers: options.body === undefined ? undefined : { "Content-Type": "application/json" },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (error) {
    if (error instanceof Error && (error.name === "TimeoutError" || error.name === "AbortError")) {
      throw new Error("The Akidev API took too long to respond.");
    }
    throw new Error("Could not reach the Akidev API.");
  }

  if (!response.ok) {
    const body: unknown = await response.json().catch(() => null);
    const message = body && typeof body === "object" && "error" in body ? (body as { error: unknown }).error : null;
    throw new ApiError(
      typeof message === "string" && message ? message : `The Akidev API returned ${response.status}.`,
      response.status,
    );
  }

  return (await response.json()) as T;
}

export function get<T>(path: string, params?: Params): Promise<T> {
  return request<T>(path, { params });
}

export function post<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, { method: "POST", body });
}
