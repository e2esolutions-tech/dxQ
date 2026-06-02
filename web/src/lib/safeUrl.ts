// Guard for URLs that come from API responses and get placed in an
// anchor `href`. Source URLs in the compatibility matrix and the
// deadline table are operator-/server-supplied; if a value ever
// carried a `javascript:` (or `data:`) scheme it would execute on
// click. We only let http(s) through; anything else collapses to
// `undefined`, which renders an inert anchor.

export function safeHttpUrl(url: string | null | undefined): string | undefined {
  if (!url) return undefined;
  let parsed: URL;
  try {
    parsed = new URL(url, window.location.origin);
  } catch {
    return undefined;
  }
  return parsed.protocol === "https:" || parsed.protocol === "http:"
    ? url
    : undefined;
}
