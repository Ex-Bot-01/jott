import { describe, expect, it } from 'vitest'
import { notifyBody, shouldNotify } from './notify'

describe('deciding to send an OS notification', () => {
  it('stays quiet while the window has focus', () => {
    // The banner is already on the screen they are looking at. A second copy of it is noise (§4).
    expect(shouldNotify(true, 'granted')).toBe(false)
  })

  it('stays quiet when the OS will not allow it', () => {
    expect(shouldNotify(false, 'denied')).toBe(false)
  })

  it('stays quiet until permission is actually granted', () => {
    // Asking is the component's job; this function only reads the answer it got back.
    expect(shouldNotify(false, 'not-determined')).toBe(false)
  })

  it('speaks once when nobody is looking and the OS allows it', () => {
    expect(shouldNotify(false, 'granted')).toBe(true)
  })
})

describe('the notification’s line', () => {
  it('names the model and what changed, without inventing anything else', () => {
    // No size: a taskbar line has nothing to measure against, and np's number belongs in the window.
    expect(notifyBody('embeddinggemma-2')).toBe('Jott: embeddinggemma-2 is ready — search now matches meanings too')
  })
})
