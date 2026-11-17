/**
 * Which marks hold the caret — the difference between a toolbar that shows formatting and a toolbar that lies.
 *
 * The inline half asks CodeMirror's parser, because it is the only thing in the window that knows what the markdown
 * actually says: `_i_` inside `**b**` is italic *and* bold, and no amount of string scanning around the caret will
 * tell you that without re-implementing the parser badly. The block half reads the line's prefix instead, because
 * `Task` nodes only exist when the GFM extension is enabled and this language setup does not enable it — a checkbox
 * is `- [ ] ` at the start of a line here, and saying so is honest where "the tree has no Task node" would be a bug
 * report about someone else's parser.
 */
import { syntaxTree } from '@codemirror/language'
import type { EditorState } from '@codemirror/state'
import type { MarkKind } from './markdownEdit'

/** The order `activeMarks` returns kinds in, so a test that compares an array reads as a sentence. */
const ORDER: readonly MarkKind[] = ['bold', 'italic', 'code', 'link', 'bullet', 'task', 'heading']

/** Parser node names to marks. Verified against the installed `@codemirror/lang-markdown` 6.5.2 rather than remembered. */
const NODE_KIND: Record<string, MarkKind> = {
  StrongEmphasis: 'bold',
  Emphasis: 'italic',
  InlineCode: 'code',
  Link: 'link',
}

const BULLET_LINE = /^\s*[-*+]\s+/
const TASK_LINE = /^\s*[-*+]\s+\[[ xX]\]\s+/
const HEADING_LINE = /^#{1,6}\s/

/** The marks holding the selection's anchor. Empty means every button is off, which is also information. */
export function activeMarks(state: EditorState): MarkKind[] {
  const pos = state.selection.main.from
  const found = new Set<MarkKind>()

  // Strictly containing, not merely touching: a caret at the end of `**bold**` sits *after* the mark, and lighting
  // Bold there would tell the next reader that something is bold which is not. The node type comes from Lezer via
  // inference — `@codemirror/language` re-exports no name for it, so annotating would mean importing a transitive
  // dependency this project does not declare.
  let node = syntaxTree(state).resolve(pos, -1)
  while (node) {
    if (node.from < pos && node.to > pos) {
      const kind = NODE_KIND[node.name]
      if (kind) found.add(kind)
    }
    // Stepping to the parent only when there is one keeps the variable non-null, which is the difference between
    // this compiling and a type annotation on a Lezer class that `@codemirror/language` does not re-export.
    const parent = node.parent
    if (!parent) break
    node = parent
  }

  const lineBlock = state.doc.lineAt(pos)
  const line = lineBlock.text
  if (state.selection.main.empty) emptyPairAt(line, pos - lineBlock.from, found)
  if (BULLET_LINE.test(line)) found.add('bullet')
  if (TASK_LINE.test(line)) found.add('task')
  if (HEADING_LINE.test(line)) found.add('heading')

  return ORDER.filter((kind) => found.has(kind))
}

/** The caret sitting inside a mark with nothing in it yet — `**|`, `_|`, `` | ``, and the empty parentheses of a
 *  fresh link. The parser reports nothing here, because an empty span is not emphasis as far as CommonMark is
 *  concerned, so this is the one place in the file that reads characters rather than asking the tree.
 *
 *  It earns its keep on the press: Bold inserts the pair and parks the caret between them, and a button that returns
 *  to "off" the instant you press it tells the reader the press did nothing. */
function emptyPairAt(line: string, col: number, into: Set<MarkKind>): void {
  const around = (open: string, close: string) => line.slice(col - open.length, col) === open && line.slice(col, col + close.length) === close
  if (around('**', '**')) into.add('bold')
  if (around('_', '_')) into.add('italic')
  if (around('`', '`')) into.add('code')
  // A `()` is only a link waiting for its URL when a `[label]` sits in front of it — otherwise every pair of
  // parentheses in a sentence would light the Link button.
  if (line.slice(col - 1, col + 1) === '()') {
    const labelEnd = line.lastIndexOf(']', col - 2)
    if (labelEnd >= 0 && line.lastIndexOf('[', labelEnd) >= 0) into.add('link')
  }
}
