/**
 * Markdown that reads like markdown, while still being the text you type.
 *
 * The alternative — a rendered preview beside the buffer — halves the typing width and needs an HTML renderer
 * to sanitise (§20's threat model does not want a third-party renderer between someone's notes and their screen).
 * This gets most of the way there with no new moving part: headings take size, `**this**` goes bold with its
 * asterisks faded, inline code gets a tint, links get an underline. The bytes in the buffer never change, so
 * nothing can drift out of sync with the file, and a note that is 40 KB of nested lists still costs one parse.
 *
 * The classes live here rather than inline so the pairing between a token and its class is testable, and so
 * `Editor.svelte` can stay about editing.
 */
import { HighlightStyle } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

/** Every class this style uses. `Editor.svelte` must style each one — a test refuses the difference, because
 *  a class with no rule behind it renders as plain text and looks like the feature was never built. */
export const MARKDOWN_CLASSES = {
  heading1: 'md-h1',
  heading2: 'md-h2',
  heading3: 'md-h3',
  heading4: 'md-h4',
  heading5: 'md-h5',
  heading6: 'md-h6',
  strong: 'md-strong',
  emphasis: 'md-em',
  strike: 'md-strike',
  code: 'md-code',
  link: 'md-link',
  quote: 'md-quote',
  list: 'md-list',
  /** The markers themselves: `#`, `**`, `` ` ``, `>`. */
  meta: 'md-meta',
} as const

export const markdownStyle = HighlightStyle.define([
  { tag: t.heading1, class: MARKDOWN_CLASSES.heading1 },
  { tag: t.heading2, class: MARKDOWN_CLASSES.heading2 },
  { tag: t.heading3, class: MARKDOWN_CLASSES.heading3 },
  { tag: t.heading4, class: MARKDOWN_CLASSES.heading4 },
  { tag: t.heading5, class: MARKDOWN_CLASSES.heading5 },
  { tag: t.heading6, class: MARKDOWN_CLASSES.heading6 },
  { tag: t.strong, class: MARKDOWN_CLASSES.strong },
  { tag: t.emphasis, class: MARKDOWN_CLASSES.emphasis },
  { tag: t.strikethrough, class: MARKDOWN_CLASSES.strike },
  { tag: t.monospace, class: MARKDOWN_CLASSES.code },
  { tag: [t.link, t.url], class: MARKDOWN_CLASSES.link },
  { tag: t.quote, class: MARKDOWN_CLASSES.quote },
  { tag: t.list, class: MARKDOWN_CLASSES.list },
  // Faded rather than hidden. Hiding `**` until the caret arrives on the line is the trick that makes people
  // think their markdown is broken when a decoration misses one token in an odd nesting; a faint marker says
  // "this is syntax" and is never wrong about what the bytes are.
  { tag: [t.meta, t.processingInstruction], class: MARKDOWN_CLASSES.meta },
])
