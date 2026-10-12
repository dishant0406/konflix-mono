import axios from 'axios'

// Shared HTTP client. Every outbound request gets a timeout and a normal-looking
// user agent, because several of these hosts reject requests from unknown clients.
export const http = axios.create({
  timeout: 20000,
  headers: {
    'User-Agent': 'konflix/2.0',
  },
})
