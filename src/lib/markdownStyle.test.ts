/// <reference types="node" />
// A style that maps a token to a class nobody styled is invisible in exactly the way 0.0.3's white text was:
// the feature appears to be missing, and no error says why. So the pairing between this module and the CSS in
// `Editor.svelte` is checked here, on the bytes that ship.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { MARKDOWN_CLASSES, markdownStyle } from './markdownStyle'

const editor = readFileSync(fileURLToPath(new URL('./Editor.svelte', import.meta.url)), 'utf8')
const source = readFileSync(fileURLToPath(new URL('./markdownStyle.ts', import.meta.url)), 'utf8')

describe('the markdown style', () => {
  it('gives each heading level its own class', () => {
    const headings = [1, 2, 3, 4, 5, 6].map((level) => (MARKDOWN_CLASSES as Record<string, string>)['heading' + level])
    expect(new Set(headings).size, 'four levels sharing one class is a document with no shape left').toBe(6)
  })

  it('fades the markers instead of leaving them at full ink', () => {
    const rule = editor.slice(editor.indexOf(`:global(.${MARKDOWN_CLASSES.meta})`))
    expect(rule, 'Editor.svelte never styles the marker class').not.toBe('')
    expect(rule).toMatch(/color:\s*color-mix\(in srgb, currentColor 4\d%/)
  })

  it('styles every class it names', () => {
    for (const [token, className] of Object.entries(MARKDOWN_CLASSES)) {
      expect(editor, `nothing styles .${className}, so ${token} would render as plain text`).toContain(
        `:global(.${className})`,
      )
    }
  })

  it('maps a token to every class it names', () => {
    // The other half of the pairing: a class styled in CSS that nothing ever assigns is dead weight that hides
    // the fact a token was never wired up. Checked on source, because `HighlightStyle`'s own spec is not public
    // and a test should not lean on a bundler's property names.
    for (const [token, className] of Object.entries(MARKDOWN_CLASSES)) {
      expect(source, `no markdown token maps to .${className}`).toContain(`MARKDOWN_CLASSES.${token}`)
    }
    expect(markdownStyle).toBeTruthy()
  })
})
