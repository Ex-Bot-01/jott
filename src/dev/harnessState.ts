//! The state behind the browser harness (`npm run ui`) — **never part of a shipped build**.
//!
//! Why this exists: nothing on the machines that build Jott can display it. `apps/shell` needs webkit2gtk or WebView2,
//! and even where those are present there is no GPU, so WebKitGTK aborts its render process and paints nothing. That
//! left every visual question — is this label clipped, is that bar too heavy, what does dark mode look like — answerable
//! only by somebody else's screenshot, days later. This harness runs the *real* `App.svelte`, `Tree.svelte`,
//! `Editor.svelte` and stylesheet against an in-memory vault in a plain browser, so a person with a display (or a
//! screenshot and a model that can read one) can look at the window.
//!
//! What it is not: a substitute for the app. The commands here answer with fixtures; there is no disk, no np, no
//! watcher. Anything this harness says about *behaviour* has to be re-checked against core's tests or a real run — its
//! only authority is over what the window looks like and how it responds to input.

import type { EntryKind, NodeEntry, StampState } from '../lib/ipc'

/** Which story the harness tells. `?s=np-missing` and friends are in the URL so a screenshot names its own scenario. */
export type Scenario = 'empty' | 'folder' | 'indexing' | 'np-missing' | 'consent'

export function scenarioFrom(search: string): Scenario {
  const q = new URLSearchParams(search).get('s')
  return q === 'empty' || q === 'indexing' || q === 'np-missing' || q === 'consent' ? q : 'folder'
}

interface File extends NodeEntry {
  text: string
  /** What the file held on "disk" — `read_note` reports this so §14's outside-change path is reachable. */
  diskText: string
  /** Every file the harness pretends exists has a real mtime; core says so too (`diskMtimeMs` is not nullable). */
  diskMtime: number
  readonly?: boolean
}

const DAY = 86_400_000
const now = () => Date.now()

function note(rel: string, text: string, state: StampState | null = 'Indexed'): File {
  return { rel, kind: 'Note', size: text.length, mtimeMs: now() - DAY, state, text, diskText: text, diskMtime: now() - DAY }
}

/** A vault shaped like the one used to test on Windows: two notes at the root, a folder with a dated note, np's own
 *  catalogue, and one attachment that must never look like a note. */
export function fixtureVault(): File[] {
  return [
    { rel: 'INDEX.md', kind: 'Generated', size: 4120, mtimeMs: now() - 60_000, state: null, text: '# Index\n\nnp publishes this; Jott shows it read-only.', diskText: '', diskMtime: now() - 60_000, readonly: true },
    note('kestrel.md', '# Kestrel notes\n\nThe **kestrel** sat on the gatepost, hunting. Grandmother called it a stormcock.\n\nSee [field log](field-log/2026-11-14.md).\n'),
    note('stormcock.md', '# Stormcock\n\nOld name for the merlin or the kestrel, depending on the county.\n', 'Embedded'),
    { rel: 'field-log', kind: 'Folder', size: 0, mtimeMs: now() - DAY, state: null, text: '', diskText: '', diskMtime: now() - DAY },
    note('field-log/2026-11-14.md', '# Field log — 14 November\n\nWind from the east. Two buzzards over the plantation.\n\nMentioned *grandma* in passing; the stormcock again at 16:40.\n'),
    note('field-log/2026-11-15.md', '# Field log — 15 November\n\nFrost. No kestrel. The buzzards came back before eight.\n', 'Waiting'),
    { rel: 'field-log/gatepost.png', kind: 'Image', size: 240_118, mtimeMs: now() - DAY, state: null, text: '', diskText: '', diskMtime: now() - DAY },
    { rel: 'scratch.txt', kind: 'Attachment', size: 82, mtimeMs: now() - 3 * DAY, state: null, text: 'not a note', diskText: 'not a note', diskMtime: now() - 3 * DAY },
  ]
}

/** The in-memory vault. Mutations from the toolbar and the editor land here, so a click visibly does something. */
export const vault = {
  open: false,
  path: '/home/example/Notes',
  files: [] as File[],
  warnings: [] as string[],
  find(rel: string): File | undefined {
    return this.files.find((f) => f.rel === rel)
  },
  touch(f: File, text: string) {
    f.text = text
    f.diskText = text
    f.size = text.length
    f.mtimeMs = now()
    f.diskMtime = now()
    f.state = 'Waiting'
  },
}

/** A tiny event bus so `npm run ui` can also drive the status line: `__harness.emit(...)` from the console. */
type Handler = (payload: unknown) => void
const listeners = new Map<string, Set<Handler>>()

export const bus = {
  emit(event: string, payload: unknown) {
    for (const h of listeners.get(event) ?? []) h(payload)
  },
  listen(event: string, handler: Handler): () => void {
    const set = listeners.get(event) ?? new Set<Handler>()
    set.add(handler)
    listeners.set(event, set)
    return () => set.delete(handler)
  },
}

/** `list_vault`'s rows are the entries minus the folder bookkeeping core does; the harness keeps them identical. */
export function listing(): { vault: string; nodes: NodeEntry[]; warnings: string[] } {
  const nodes = vault.files.map(({ text: _t, diskText: _d, readonly: _r, ...entry }) => entry)
  return { vault: vault.path, nodes, warnings: vault.warnings }
}
