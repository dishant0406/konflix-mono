// Torrent stream source: this server joins the swarm and relays the bytes.
//
// Every byte a viewer watches crosses this server, which is fine for a small
// deployment and expensive at any real scale. It is kept because it needs no
// account, no third party, and works when no debrid service has the release
// cached, so it is the fallback that makes the debrid source safe to depend on.

import { config } from '../../../lib/config.js'
import { openRelease } from '../torrent.js'
import { isDirectlyPlayable, canRemux } from '../media.js'

function playableUrl(infoHash) {
  return `${config.publicUrl}/stream/${infoHash}`
}

export default {
  name: 'torrent',

  ready() {
    return true
  },

  /**
* @param {{ episodeNumber: number, season: number|null, ranked: Array, deadlineMs: number }} input
   * @returns {Promise<{ available: boolean, url?: string, streamType?: string,
   *                     release?: string, reason?: string }>}
   */
  async resolve({ episodeNumber, season = null, ranked, deadlineMs }) {
    const failures = []
    const deadline = Date.now() + deadlineMs

    for (const release of ranked.slice(0, config.maxCandidates)) {
      // A run of dead swarms must not keep the request open indefinitely, so each
      // candidate gets only the time left in the overall budget.
      const remaining = deadline - Date.now()
      if (remaining <= 0) {
        failures.push('deadline reached')
        break
      }

      try {
const session = await openRelease(release, episodeNumber, remaining, season)
        if (!isDirectlyPlayable(session.info) && !canRemux(session.info)) {
          failures.push(`${release.title}: unsupported ${session.info.videoCodec}`)
          continue
        }

        return {
          available: true,
          // One URL for both cases. /stream/<hash> answers with MP4 bytes when the file
          // is directly playable and with an HLS playlist when it is not, so the player
          // never has to know which it got.
          url: playableUrl(release.infoHash),
          streamType: isDirectlyPlayable(session.info) ? 'file' : 'hls',
          release: release.title,
        }
      } catch (error) {
        failures.push(`${release.title}: ${error.message}`)
      }
    }

    // The detail is logged rather than returned: it names releases and upstream
    // errors, and the viewer can act on none of it.
    if (failures.length > 0) console.warn(`[stream:torrent] ${failures.join(' | ')}`)
    return { available: false, reason: 'Found releases, but none could be played.' }
  },
}
