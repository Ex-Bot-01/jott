// What the editor's save state is, as one pure function of what the buffer holds and what the disk
// says. The components ask this question; they do not answer it.
//
// Two spec rules meet here. §11: a save whose bytes did not change triggers nothing — np would answer
// "nothing owed" and the round trip would be the only thing that happened. §14: unsaved typing is never
// overwritten without asking, and the app does not pick a side in a conflict the user has not seen yet.

export type SavePhase = 'clean' | 'dirty' | 'saving' | 'saved' | 'conflict'

export interface SaveInput {
  /** The buffer holds text the file does not. */
  dirty: boolean
  /** The file's bytes differ from what was read into this buffer, or from what we last wrote. */
  diskChanged: boolean
  /** A write is in flight. */
  saving: boolean
  /** The hash of the bytes our last completed write sent. Absent until this session has written. */
  savedHash?: string
  /** The hash of what the buffer holds now. */
  currentHash: string
}

export function saveState(input: SaveInput): SavePhase {
  // A save in flight outranks everything, including a "changed on disk" signal that our own write is
  // almost always the cause of. Calling that a conflict would train someone to wave the warning away.
  if (input.saving) return 'saving'
  // §14: something else moved the file while this buffer holds typing nobody has saved.
  if (input.diskChanged && input.dirty) return 'conflict'
  if (input.dirty) return 'dirty'
  // "Saved" is a claim about the disk, so it is made only about bytes we can name: the ones this app
  // wrote and can still match against what is there.
  if (input.savedHash !== undefined && input.savedHash === input.currentHash) return 'saved'
  // Includes the case that looks like "saved" and is not: our write landed, then something else rewrote
  // the file. The buffer is still what it was, so nothing of ours is lost — but the app does not vouch
  // for content it did not see land (§4).
  return 'clean'
}

/** Whether Ctrl+S (or autosave) should write at all. */
export function wantsSave(input: SaveInput): boolean {
  // `dirty` is the only phase that means "there are bytes to send and nothing standing in the way":
  // not while a write is in flight, and not over a conflict the user has not been shown (§14).
  return saveState(input) === 'dirty'
}
