// The one place "did the content change?" is decided (§11), and the one place §14's "never overwrite
// unsaved work" turns into a word the interface can act on. Kept out of the components so it has tests
// instead of a comment in a template.

import { describe, expect, it } from 'vitest'
import { saveState, wantsSave } from './saveState'

describe('saveState', () => {
  it('is clean when nobody has typed since the file was read', () => {
    expect(saveState({ dirty: false, diskChanged: false, saving: false, currentHash: 'aaa' })).toBe('clean')
  })

  it('is dirty once there is something the file does not have', () => {
    expect(saveState({ dirty: true, diskChanged: false, saving: false, currentHash: 'bbb' })).toBe('dirty')
  })

  it('is saving while the write is in flight, typed text and all', () => {
    // The spinner is not a conflict, and it must not invite a second Ctrl+S to queue another write.
    expect(saveState({ dirty: true, diskChanged: false, saving: true, currentHash: 'bbb' })).toBe('saving')
  })

  it('is saved when the bytes on disk are the bytes we wrote', () => {
    expect(
      saveState({ dirty: false, diskChanged: false, saving: false, savedHash: 'ccc', currentHash: 'ccc' }),
    ).toBe('saved')
  })

  it('is not "saved" when what landed on disk is not what we wrote', () => {
    // Somebody's sync client replaced our bytes between the write and the read. Saying "saved" here
    // would be the app vouching for content it did not see land (§4).
    expect(saveState({ dirty: false, diskChanged: true, saving: false, savedHash: 'ccc', currentHash: 'ddd' })).toBe('clean')
  })

  it('is a conflict when the file moved under unsaved typing', () => {
    expect(saveState({ dirty: true, diskChanged: true, saving: false, currentHash: 'eee' })).toBe('conflict')
  })

  it('is not a conflict when the buffer has nothing of ours to lose', () => {
    // §14: a clean buffer follows the disk. The reload is Task 8's `decide`, and this is already its
    // answer — nothing to warn about.
    expect(saveState({ dirty: false, diskChanged: true, saving: false, savedHash: 'fff', currentHash: 'ggg' })).toBe('clean')
  })

  it('ranks a save in flight above a conflict it raced with', () => {
    // Our own write is what moved the file's timestamp. Reporting "conflict" for our own save would
    // train someone to dismiss the warning that matters (§14).
    expect(saveState({ dirty: true, diskChanged: true, saving: true, currentHash: 'hhh' })).toBe('saving')
  })
})

describe('wantsSave', () => {
  it('does not write an unmodified buffer at all', () => {
    // §11: Ctrl+S on text that has not changed triggers no write and no ingest pass — np would answer
    // "nothing owed" and the round trip would be the only thing that happened.
    expect(wantsSave({ dirty: false, diskChanged: false, saving: false, savedHash: 'aaa', currentHash: 'aaa' })).toBe(false)
  })

  it('writes what was typed', () => {
    expect(wantsSave({ dirty: true, diskChanged: false, saving: false, currentHash: 'bbb' })).toBe(true)
  })

  it('does not write twice because Ctrl+S was enthusiastic', () => {
    expect(wantsSave({ dirty: true, diskChanged: false, saving: true, currentHash: 'bbb' })).toBe(false)
  })

  it('never writes over a conflict without being asked', () => {
    // §14: the app keeps the buffer and says so. Writing here would choose a side in a conflict the
    // user has not been shown yet.
    expect(wantsSave({ dirty: true, diskChanged: true, saving: false, currentHash: 'eee' })).toBe(false)
  })
})
