// Helpers for working with magnet links and release sizes.

// A 20-byte info hash is written either as 40 hex characters or as 32 base32
// characters, and indexers use both. webtorrent always reports the hex form, so
// everything here is normalised to hex: the stream URL, the torrent map key and
// the eviction lookups must agree, or a torrent is added under one form and can
// never be found (and therefore never evicted) under the other.
const BASE32_ALPHABET = 'abcdefghijklmnopqrstuvwxyz234567'

function base32ToHex(value) {
  let buffer = 0
  let bits = 0
  const bytes = []

  for (const character of value.toLowerCase()) {
    const index = BASE32_ALPHABET.indexOf(character)
    if (index === -1) return null
    buffer = (buffer << 5) | index
    bits += 5
    if (bits >= 8) {
      bits -= 8
      bytes.push((buffer >> bits) & 0xff)
    }
  }

  return bytes.length === 20 ? Buffer.from(bytes).toString('hex') : null
}

export function toHexInfoHash(value) {
  if (!value) return null
  const hash = value.toLowerCase()
  if (/^[0-9a-f]{40}$/.test(hash)) return hash
  if (/^[a-z2-7]{32}$/.test(hash)) return base32ToHex(hash)
  return null
}

export function infoHashFromMagnet(magnet) {
  const match = /xt=urn:btih:([a-zA-Z0-9]+)/.exec(magnet || '')
  return match ? toHexInfoHash(match[1]) : null
}

// Magnet URIs keep the `xt` value literal; only the display name and trackers are
// percent-encoded. URLSearchParams would encode the ":" in "urn:btih:", which many
// clients (and our own parser) do not expect.
export function buildMagnet(infoHash, name, trackers) {
  const parts = [`xt=urn:btih:${infoHash}`]
  if (name) parts.push(`dn=${encodeURIComponent(name)}`)
  for (const tracker of trackers) parts.push(`tr=${encodeURIComponent(tracker)}`)
  return `magnet:?${parts.join('&')}`
}

const UNITS = {
  b: 1,
  kb: 1024,
  kib: 1024,
  mb: 1024 ** 2,
  mib: 1024 ** 2,
  gb: 1024 ** 3,
  gib: 1024 ** 3,
  tb: 1024 ** 4,
  tib: 1024 ** 4,
}

// Turns "1.309 GB" or "700 MiB" into a byte count.
export function parseSize(text) {
  if (!text) return null
  const match = /([\d.]+)\s*([a-z]+)/i.exec(text)
  if (!match) return null
  const value = Number.parseFloat(match[1])
  const unit = UNITS[match[2].toLowerCase()]
  if (!Number.isFinite(value) || !unit) return null
  return Math.round(value * unit)
}
