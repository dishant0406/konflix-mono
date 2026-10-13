import axios from 'axios'

// Every outbound request gets a timeout and a normal-looking user agent, because several
// of these hosts reject requests from unknown clients. The agent is exported because
// ffmpeg is another outbound client and must send the same one.
export const USER_AGENT = 'konflix/2.0'

// Shared HTTP client.
export const http = axios.create({
  timeout: 20000,
  headers: {
    'User-Agent': USER_AGENT,
  },
})
