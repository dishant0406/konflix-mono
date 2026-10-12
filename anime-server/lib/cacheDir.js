// Filesystem locations and cleanup.
//
// webtorrent writes downloaded pieces to disk, so an unbounded cache would
// eventually fill the server. The cache is wiped on startup, which is safe
// because everything in it can be re-downloaded.

import fs from 'node:fs'
import path from 'node:path'
import { config } from './config.js'

export const cacheDir = path.resolve(config.cacheDir)

export function prepareCacheDir() {
  fs.rmSync(cacheDir, { recursive: true, force: true })
  fs.mkdirSync(cacheDir, { recursive: true })
}
