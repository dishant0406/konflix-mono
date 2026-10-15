// The single GraphQL client used by the whole app.
//
// The API is one endpoint with one shape: POST { query, variables } and read
// `data`. A full Apollo runtime is not needed for that, so this is a small fetch
// wrapper, which also keeps the request count and the bundle honest.

const URI = process.env.NEXT_PUBLIC_GRAPHQL_URI || "http://localhost:9000/graphql";

// Resolving a stream searches torrent indexers and can legitimately take a while,
// but it must never hang. The API bounds its own work with RESOLVE_DEADLINE_MS
// (75s by default), so this sits just above that and turns a stuck request into a
// visible error instead of a permanent spinner.
const TIMEOUT_MS = Number(process.env.NEXT_PUBLIC_GRAPHQL_TIMEOUT_MS) || 90000;

export type Variables = Record<string, unknown>;

/**
 * A parsed GraphQL document. `graphql-tag` is not used: the API accepts a query
 * string, and keeping the documents as strings means there is no second GraphQL
 * runtime in the browser bundle.
 */
export type DocumentNode = string;

export async function request<T>(query: DocumentNode, variables?: Variables): Promise<T> {
  let response: Response;
  try {
    response = await fetch(URI, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ query, variables }),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (error) {
    if (error instanceof Error && (error.name === "TimeoutError" || error.name === "AbortError")) {
      throw new Error("The Akidev API took too long to respond.");
    }
    throw new Error("Could not reach the Akidev API.");
  }

  if (!response.ok) {
    throw new Error(`The Akidev API returned ${response.status}.`);
  }

  const body = await response.json();
  if (body.errors?.length) {
    throw new Error(body.errors.map((error: { message: string }) => error.message).join("; "));
  }

  return body.data as T;
}

