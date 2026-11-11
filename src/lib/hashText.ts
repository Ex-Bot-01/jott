// SHA-256 of the buffer's text, in the same spelling the backend uses (`sha256_hex`: lowercase hex of
// the UTF-8 bytes). The editor needs it to answer "did my bytes land?" (§11) and "is this a conflict?"
// (§14) with the same numbers Rust produces, not with a string-length heuristic.

/** WebCrypto's own digest over UTF-8 bytes — the identical input `jott-core` hashes on disk. */
export async function hashText(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}
