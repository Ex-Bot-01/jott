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
      title: node.kind === 'Generated' ? GENERATED_TITLE : marker.title,
      greyed: node.kind === 'Generated',
    })

    if (isFolder && !isOpen) hiddenBelow = node.rel
  }
  return rows
}

/** The §9 legend. `Indexed` and `Waiting` share `●` — both mean something is owed — and say which in
 * the tooltip, because two dots that differ only in weight read as a rendering glitch. */
function markerFor(state: StampState | null): { marker: string; title: string } {
  if (state === null || state === undefined) return { marker: '', title: '' }
  if (state === 'Embedded') return { marker: '·', title: 'embedded' }
  // Two dots that both said "waiting" — for the index, and to embed — described opposite states, which is how
  // a tester ended up reporting that the app contradicted itself. The words now say what each state *is*.
  if (state === 'Indexed') return { marker: '●', title: 'indexed · awaiting embedding' }
  // Not "never indexed": `Waiting` also means "changed since np last saw it" (`stamps.rs`). The plan's word
  // for both is *waiting*, and saying more than that would be a claim about history nobody has (§4).
  if (state === 'Waiting') return { marker: '○', title: 'not indexed yet' }
  if (typeof state === 'object' && 'Refused' in state) {
    // §18.5: when np gave no reason, the marker says refused and stops there.
    return { marker: '!', title: state.Refused ? `refused · ${state.Refused}` : 'refused' }
  }
  // A state from a newer core than this file knows: draw nothing rather than guess (§4).
  return { marker: '', title: '' }
}

/** What the greyed `INDEX.md` row says when you hover it. The file is np's, not yours and not Jott's, and a
    person who has never seen one appear in their folder deserves to be told that in a sentence. */
export const GENERATED_TITLE = 'Catalogue written by np — Jott never edits it'

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
