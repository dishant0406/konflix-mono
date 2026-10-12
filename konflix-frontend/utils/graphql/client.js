// The single GraphQL client used by every page.
//
// This replaces apollo-boost with a small fetch-based client. The app only ever
// needed `client.query({ query, variables })` and `gql`, so a full Apollo runtime
// was unnecessary weight.

import { print } from 'graphql'

const URI = process.env.NEXT_PUBLIC_GRAPHQL_URI || 'http://localhost:9000/graphql'

// Resolving a stream searches torrent indexers and can legitimately take a while,
// but it must never hang. The server bounds its own work with RESOLVE_DEADLINE_MS
// (75s by default), so this sits just above that and turns a stuck request into a
// visible error instead of a permanent spinner.
const TIMEOUT_MS = Number(process.env.NEXT_PUBLIC_GRAPHQL_TIMEOUT_MS) || 90000

export async function query({ query, variables }) {
  let response
  try {
    response = await fetch(URI, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query: print(query), variables }),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    })
  } catch (error) {
    if (error.name === 'TimeoutError' || error.name === 'AbortError') {
      throw new Error('The Konflix API took too long to respond.')
    }
    throw new Error('Could not reach the Konflix API.')
  }

  if (!response.ok) {
    throw new Error(`GraphQL request failed with status ${response.status}`)
  }

  const body = await response.json()
  if (body.errors?.length) {
    throw new Error(body.errors.map((error) => error.message).join('; '))
  }

  return { data: body.data }
}

export { gql } from 'graphql-tag'
