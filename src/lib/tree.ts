// Which tree rows are visible, how deep they sit, and what their §9 marker means. The walk in
// `jott-core` already excluded dot-names and sorted by `rel` bytes; this file decides nothing about the
// vault — it only draws it.

import type { NodeEntry, StampState } from './ipc'

export interface Row {
  rel: string
  name: string
  kind: NodeEntry['kind']
  depth: number
  /** A folder with something in it: the only row that gets a twisty. */
  hasChildren: boolean
  /** `●` owed, `·` embedded, `!` refused, `` nothing to show. §9's legend, §11's states. */
  marker: string
  /** Which colour that mark wears, named here rather than in the stylesheet: a class CSS invents and this file
      never emits is a rule nobody can see (we have been burned by exactly that twice — `.greyed` with its rule
      deleted, and `--cm-cursor`, a variable CodeMirror does not have). */
  tone: MarkerTone
  /** The tooltip. For a refusal this is where np's reason goes, because the row has no room for it. */
  title: string
  /** np's own catalog files: visible, greyed, never edited here (§9). */
  greyed: boolean
}

export function flattenTree(nodes: NodeEntry[], open: Set<string>): Row[] {
  const rows: Row[] = []
  // A row is visible when every folder above it is open. Computed as we go, because the walk's order
  // puts a parent before its children.
  let hiddenBelow: string | null = null

  for (const node of nodes) {
    if (hiddenBelow !== null && !node.rel.startsWith(`${hiddenBelow}/`)) {
      hiddenBelow = null
    }
    if (hiddenBelow !== null) continue

    const depth = node.rel.split('/').length - 1
    const isFolder = node.kind === 'Folder'
    const isOpen = open.has(node.rel)
    const marker = markerFor(node.state)
    rows.push({
      rel: node.rel,
      name: node.rel.slice(node.rel.lastIndexOf('/') + 1),
      kind: node.kind,
      depth,
      hasChildren: isFolder && nodes.some((other) => other.rel.startsWith(`${node.rel}/`)),
      marker: marker.marker,
      tone: marker.tone,
      title: node.kind === 'Generated' ? GENERATED_TITLE : marker.title,
      greyed: node.kind === 'Generated',
    })

    if (isFolder && !isOpen) hiddenBelow = node.rel
  }
  return rows
}

/** The five looks a marker can have. `none` is drawn as nothing at all: a folder, or a state from a newer core
    than this file knows, has no claim to make about indexing. */
export type MarkerTone = 'embedded' | 'queued' | 'waiting' | 'refused' | 'none'

/** The §9 legend. `Indexed` and `Waiting` share `●` — both mean something is owed — and say which in
 * the tooltip, because two dots that differ only in weight read as a rendering glitch. */
export function markerFor(state: StampState | null): { marker: string; tone: MarkerTone; title: string } {
  if (state === null || state === undefined) return { marker: '', tone: 'none', title: '' }
  if (state === 'Embedded') return { marker: '·', tone: 'embedded', title: 'embedded' }
  // Two dots that both said "waiting" — for the index, and to embed — described opposite states, which is how
  // a tester ended up reporting that the app contradicted itself. The words now say what each state *is*.
  if (state === 'Indexed') return { marker: '●', tone: 'queued', title: 'indexed · awaiting embedding' }
  // Not "never indexed": `Waiting` also means "changed since np last saw it" (`stamps.rs`). The plan's word
  // for both is *waiting*, and saying more than that would be a claim about history nobody has (§4).
  if (state === 'Waiting') return { marker: '○', tone: 'waiting', title: 'not indexed yet' }
  if (typeof state === 'object' && 'Refused' in state) {
    // §18.5: when np gave no reason, the marker says refused and stops there.
    return { marker: '!', tone: 'refused', title: state.Refused ? `refused · ${state.Refused}` : 'refused' }
  }
  // A state from a newer core than this file knows: draw nothing rather than guess (§4).
  return { marker: '', tone: 'none', title: '' }
}

/** One sample per colour, and the legend's words are `markerFor`'s own sentences for them — so the key beside
    the tree and the tooltip on a row cannot drift into two descriptions of one dot. */
const LEGEND_SAMPLES: { tone: Exclude<MarkerTone, 'none'>; sample: StampState }[] = [
  { tone: 'embedded', sample: 'Embedded' },
  { tone: 'queued', sample: 'Indexed' },
  { tone: 'waiting', sample: 'Waiting' },
  { tone: 'refused', sample: { Refused: '' } },
]

export const MARKER_LEGEND = LEGEND_SAMPLES.map((entry) => ({
  ...entry,
  marker: markerFor(entry.sample).marker,
  word: markerFor(entry.sample).title,
}))

/** What the greyed `INDEX.md` row says when you hover it. The file is np's, not yours and not Jott's, and a
    person who has never seen one appear in their folder deserves to be told that in a sentence. */
export const GENERATED_TITLE = 'Catalogue written by note-processor — open it to read, Jott never edits it'

/** How many notes still owe their first pass: no stamp at all, or a stamp that says nobody indexed them yet.
    A refused note is not counted — it was tried, and the row already says so. */
export function notesAwaitingIndex(nodes: NodeEntry[]): number {
  return nodes.filter(
    (node) => node.kind === 'Note' && (node.state === null || node.state === undefined || node.state === 'Waiting'),
  ).length
}

/** How many notes np has indexed but never embedded. Search finds their words; it does not find their
    meanings, and a person who wonders why "kumquat" misses a note about kumquats is owed that difference in
    words rather than a dot they have to decode. */
export function notesAwaitingEmbedding(nodes: NodeEntry[]): number {
  return nodes.filter((node) => node.kind === 'Note' && node.state === 'Indexed').length
}

/** How many notes in this folder already carry vectors. The model dot counts these because "the model is on my disk"
 *  and "this folder can be searched by meaning" are different sentences, and only the second one explains why a
 *  semantic search came back empty. Counted from Jott's own per-file markers, which is what the tree shows. */
export function notesEmbedded(nodes: NodeEntry[]): number {
  return nodes.filter((node) => node.kind === 'Note' && node.state === 'Embedded').length
}

/** What a row's menu offers. Decided here rather than in the component so "you cannot delete np's own catalogue"
 *  is a fact with a test on it, not an `if` inside markup. */
export type TreeAction = 'new-note' | 'new-folder' | 'rename' | 'move' | 'delete'

export function actionsFor(row: Row | null): TreeAction[] {
  if (row === null) return ['new-note', 'new-folder']
  if (row.greyed) return [] // np wrote this file; Jott neither edits nor deletes it
  if (row.kind === 'Folder') return ['new-note', 'new-folder', 'rename', 'move', 'delete']
  return ['rename', 'move', 'delete']
}

/** The folder a new note or folder belongs to: inside the selected folder, beside the selected note, or at the
 *  root when nothing is selected. Root is `''` because that is what the backend's `rel`s are relative to. */
export function targetFolder(row: Row | null): string {
  if (row === null) return ''
  if (row.kind === 'Folder') return row.rel.endsWith('/') ? row.rel : `${row.rel}/`
  const cut = row.rel.lastIndexOf('/')
  return cut < 0 ? '' : row.rel.slice(0, cut + 1)
}

/** Where a row could be moved to.
 *
 * Excluded: its current parent (that is not a move), the folder itself and anything inside it (a folder cannot be
 * put inside its own child — the backend refuses it, and a menu that offers a refusal is a bug with extra steps).
 * Root is always a candidate, spelled `''`.
 */
/**
 * Where the toolbar's **+ note** / **+ folder** put the thing they make.
 *
 * It used to be a constant — the vault root, always — while the row menu offered *New note here* on any folder two
 * pixels away. The toolbar contradicted the menu, and a person who had just clicked a folder had no way to tell the
 * window it was the destination.
 */
export function toolbarTarget(selectedFolder: string | null): { folder: string; label: string } {
  if (!selectedFolder) return { folder: '', label: 'the vault root' }
  const folder = selectedFolder.endsWith('/') ? selectedFolder : `${selectedFolder}/`
  return { folder, label: folder.replace(/\/$/, '') }
}

export function moveTargets(rows: Row[], rel: string): string[] {
  const here = targetFolder(rows.find((row) => row.rel === rel) ?? null)
  const prefix = rel.endsWith('/') ? rel : `${rel}/`
  const folders = new Set<string>([''])
  for (const row of rows) {
    if (row.kind !== 'Folder') continue
    if (row.rel === rel || row.rel.startsWith(prefix)) continue
    folders.add(row.rel.endsWith('/') ? row.rel : `${row.rel}/`)
  }
  folders.delete(here)
  return [...folders].sort()
}

/** What the move dialog means by a typed name.
 *
 *  The dialog used to list existing folders and nothing else, so in a vault with no folders it was an empty box —
 *  "I can right click and select Move to…, but I see only an empty field". The fix is not a longer list; it is
 *  letting the destination be a folder that does not exist yet, which `create_folder` already supports. */
export type MovePlan =
  | { kind: 'move'; to: string }
  | { kind: 'create-then-move'; folder: string; to: string }
  | { kind: 'refuse'; sentence: string }

/** Turn what somebody typed into a destination, or into a sentence explaining why it is not one.
 *
 *  Existing folder → move into it. Anything else → create it, then move. Names that would leave the vault are
 *  refused here rather than at the backend, because the dialog can say "that name walks out of the folder" while
 *  the person still has the field in front of them; the guard stays in Rust too, since this function is a courtesy
 *  and not the security boundary. */
export function planMoveInto(rel: string, typed: string, folders: string[]): MovePlan {
  const raw = typed.trim()
  if (raw === '') return { kind: 'refuse', sentence: 'Name the folder to move it into.' }

  // Split on either separator rather than rewriting one into the other: a Windows path typed with backslashes
  // means the same thing, and a share path only looks like an ordinary nested name after a substitution has
  // eaten the distinction.
  const parts = raw.split(/[/\\]/).filter((part) => part !== '')
  if (parts.length === 0) return { kind: 'refuse', sentence: 'Name the folder to move it into.' }

  // A leading separator means an absolute path or a share, and neither names a folder inside this vault. Read
  // from what was typed, before the split dropped it.
  if (raw.startsWith('/') || raw.startsWith('\\')) {
    return { kind: 'refuse', sentence: 'That is a path outside the folder. Name a folder inside it, like ideas.' }
  }

  for (const part of parts) {
    // `.` and `..` are the walk out; a colon is a drive letter or an alternate data stream.
    if (part === '.' || part === '..' || part.includes(':')) {
      return { kind: 'refuse', sentence: 'That name walks out of the folder. Choose a plain name, like ideas.' }
    }
  }

  const normalised = parts.join('/')

  const folder = `${normalised}/`
  const here = rel.includes('/') ? `${rel.slice(0, rel.lastIndexOf('/') + 1)}` : ''
  if (folder === here) {
    return { kind: 'refuse', sentence: 'It is already in that folder — a move needs somewhere else to go.' }
  }

  // A destination inside the row itself. `moveTargets` omits the row and its subtree, so nothing downstream could see
  // this coming: moving the folder `work/` into `work/2026` reads as an ordinary nested name, gets *created*, and is
  // refused only by core — after the vault has an empty folder left over from a move that never happened. Saying no
  // here is what keeps half a plan from being carried out.
  if (rel.endsWith('/') && (folder === rel || folder.startsWith(rel))) {
    return { kind: 'refuse', sentence: 'That destination is inside the folder you are moving. Choose somewhere else.' }
  }

  const known = new Set(folders.map((f) => (f.endsWith('/') ? f : `${f}/`)))
  if (known.has(folder)) return { kind: 'move', to: `${folder}${nameOf(rel)}` }
  return { kind: 'create-then-move', folder, to: `${folder}${nameOf(rel)}` }
}

/** The last segment of a `rel`, without the trailing slash a folder carries. Folders are stored as `work/`, so a
 *  name taken by slicing after the final slash comes back empty — and a move built from it asks core to put `work/`
 *  at `ideas/`, which is the destination of its contents rather than of the folder. */
function nameOf(rel: string): string {
  const trimmed = rel.endsWith('/') ? rel.slice(0, -1) : rel
  return trimmed.slice(trimmed.lastIndexOf('/') + 1)
}

/** Join a `rel` onto a destination folder, keeping it a `rel`. */
export function moveTo(rel: string, folder: string): string {
  // One name rule for both halves of the dialog, so picking an existing folder and naming a new one cannot disagree
  // about what a folder is called.
  return `${folder}${nameOf(rel)}`
}
