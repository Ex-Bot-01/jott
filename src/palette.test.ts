/**
 * A stylesheet test, which is a strange thing to write down until it has cost someone an install.
 *
 * 0.0.3 said `color-scheme: light dark` and then named neither ink nor background, so WebView2 picked its own
 * text colour — white, on a dark-mode Windows desktop — while the skeleton in `index.html` painted near-white
 * paper underneath it. The app was there; every pixel of it was invisible.
 *
 * So: if a file tells the webview it handles dark mode, that file has to say what ink is, in both schemes.
 * jsdom cannot check contrast, and neither can I from here, but a test can refuse the shape that made it
 * possible.
 */
/// <reference types="node" />
// This one reads files rather than importing them, and it is deliberate: under Vitest a `.css?raw` import
// came back empty in CI while passing here, which is the wrong kind of flaky for a test whose whole job is to
// be a guard. The bytes on disk are what the bundler ships.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const appCss = readFileSync(fileURLToPath(new URL('./app.css', import.meta.url)), 'utf8')
const shell = readFileSync(fileURLToPath(new URL('../index.html', import.meta.url)), 'utf8')

/** The dark block is the `@media` rule that follows the light one, not a stray mention of the word. */
function darkBlock(css: string): string {
  const start = css.indexOf('prefers-color-scheme: dark')
  return start === -1 ? '' : css.slice(start, css.indexOf('}', css.indexOf('}', start)) + 1)
}

describe('the palette names both colours', () => {
  for (const [name, css] of [
    ['src/app.css', appCss],
    ['index.html', shell],
  ]) {
    const declares = (token: string) => new RegExp(`${token}:\\s*#[0-9a-fA-F]{3,8}`).test(css)

    it(`${name} sets ink and paper up front, and again for dark`, () => {
      expect(declares('--paper'), `${name} has no --paper`).toBe(true)
      expect(declares('--ink'), `${name} has no --ink`).toBe(true)

      const dark = darkBlock(css)
      expect(dark, `${name} never overrides the palette for a dark desktop`).toContain('--paper')
      expect(dark).toContain('--ink')
    })
  }

  it('paints the page with those colours instead of inheriting the webview’s', () => {
    const body = appCss.slice(appCss.indexOf('body {'))
    expect(body).toContain('background: var(--paper)')
    expect(body).toContain('color: var(--ink)')
  })

  it('never claims dark support without an ink to go with it', () => {
    for (const [name, css] of [
      ['src/app.css', appCss],
      ['index.html', shell],
    ]) {
      if (!css.includes('color-scheme')) continue
      expect(css, `${name} opts into dark mode without saying what ink is`).toContain('--ink')
    }
  })
})

/**
 * A caret you cannot find is not a small thing in a text editor: it is the one pixel that says where your
 * typing will land. CodeMirror draws its own default — 1px, its own grey, and `transparent` until a focus
 * selector matches — which on a scaled Windows display reads as no caret at all. The palette owns ink, so the
 * palette has to name the caret too.
 */
describe('the caret is named, not inherited', () => {
  const editor = readFileSync(fileURLToPath(new URL('./lib/Editor.svelte', import.meta.url)), 'utf8')

  it('draws the editor caret in ink, wide enough to see', () => {
    const rule = editor.slice(editor.indexOf('.cm-cursor'))
    expect(rule, 'the editor never mentions .cm-cursor').not.toBe('')
    expect(rule).toMatch(/border-left:\s*2px solid var\(--ink\)/)
  })

  it('keeps the native carets in the window on the same ruler', () => {
    expect(appCss).toMatch(/caret-color:\s*var\(--ink\)/)
  })
})
