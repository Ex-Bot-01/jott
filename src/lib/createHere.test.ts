import { describe, expect, it } from 'vitest'
import { NO_FOLDER_OPEN, createBlocked } from './createHere'

describe('asking for a note or folder with nothing open', () => {
  it('lets the intention through when a folder is open', () => {
    expect(createBlocked(true)).toBeNull()
  })

  it('refuses in words that name both the missing thing and the way out', () => {
    const blocked = createBlocked(false)
    expect(blocked).not.toBeNull()
    // The smoke test looks for this phrase; a rewording that drops it would silently un-moor that test.
    expect(NO_FOLDER_OPEN.toLowerCase()).toContain('no folder is open')
    expect(NO_FOLDER_OPEN.toLowerCase()).toContain('choose')
    expect(blocked).toBe(NO_FOLDER_OPEN)
  })

  it('does not say "nothing matched" or blame note-processor for a missing folder', () => {
    // Both sentences exist elsewhere in the window and neither belongs here: nothing was searched, and np was never
    // involved in choosing where a file goes.
    expect(NO_FOLDER_OPEN.toLowerCase()).not.toContain('note-processor')
    expect(NO_FOLDER_OPEN.toLowerCase()).not.toContain('nothing matched')
  })
})
