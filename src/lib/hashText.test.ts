// The editor's hashes have to be the *same numbers* as the backend's, or "saved" and "conflict" are
// computed with two different rulers. The literal below is `sha256sum`'s answer for `# A\n` — the fixture
// every Rust test writes — so this file fails if either side stops meaning SHA-256.

import { describe, expect, it } from 'vitest'
import { hashText } from './hashText'

describe('hashText', () => {
  it('matches what sha256 produces for a known note', async () => {
    expect(await hashText('# A\n')).toBe('aa1237b773c38dbddef583c4868aaea7a44c5237ea7923aecca5513764b42d80')
  })

  it('is lowercase hex of the UTF-8 bytes, so CRLF and LF never collide', async () => {
    const lf = await hashText('a\nb\n')
    const crlf = await hashText('a\r\nb\r\n')
    expect(lf).toMatch(/^[0-9a-f]{64}$/)
    // §13: line endings are content. A hash that normalised them would hide the only change there is.
    expect(lf).not.toBe(crlf)
  })

  it('is stable for the same text', async () => {
    expect(await hashText('same')).toEqual(await hashText('same'))
  })
})
