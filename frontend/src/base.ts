// Path prefix the app is served under, '' at the root or e.g. '/mpf' behind a
// reverse proxy that strips the prefix. index.html computes it at runtime and
// sets <base href>, so one build works under any prefix.
export const BASE = new URL(document.baseURI).pathname.replace(/\/+$/, '')
