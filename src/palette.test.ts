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
const treeSvelte = readFileSync(fileURLToPath(new URL('./lib/Tree.svelte', import.meta.url)), 'utf8')
const treeTs = readFileSync(fileURLToPath(new URL('./lib/tree.ts', import.meta.url)), 'utf8')

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
 * A caret you cannot find is not a small thing in a text editor: it is the one pixel that says where your typing will
 * land. Two releases went hunting for it in CodeMirror's drawn caret — and Jott never asks CodeMirror to draw one.
 * `drawSelection()` is the only producer of `.cm-cursor` elements (`node_modules/@codemirror/view/dist/index.js`,
 * `function drawSelection` returns `[selectionConfig, cursorLayer, selectionLayer, hideNativeSelection, …]`), it is not
 * imported anywhere in `src/`, and so every rule that coloured `.cm-cursor` matched nothing at all. The caret this window
 * has always had is the browser's own — and since c6665c7 (0.0.16) this file hid it with
 * `'.cm-content': { caretColor: 'transparent' }`, under a comment claiming CodeMirror hides the native caret itself.
 * It does not: that hiding ships inside `drawSelection()` too.
 */
describe('the caret is the browser’s, and this window stops hiding it', () => {
  const editor = readFileSync(fileURLToPath(new URL('./lib/Editor.svelte', import.meta.url)), 'utf8')

  it('names the caret colour instead of making it transparent', () => {
    // The theme is where the answer has to live: CodeMirror injects its style modules after the bundled stylesheet, so
    // a global `caret-color` in app.css loses to anything this component says about `.cm-content`.
    const theme = editor.slice(editor.indexOf('EditorView.theme'))
    expect(theme, 'the editor builds no CodeMirror theme').not.toBe('')
    expect(theme).toMatch(/'.cm-content':\s*\{[^}]*caretColor:\s*'var\(--ink\)'/)
  })

  it('never hides a caret anywhere again', () => {
    // The bug this pins was written by an attempt to fix the opposite bug. `transparent` is not a way to say "CodeMirror
    // will draw it"; when nothing draws it, it is a way to say "there is no caret".
    expect(editor).not.toMatch(/caretColor:\s*['"]transparent/)
    expect(appCss).not.toMatch(/caret-color:\s*transparent/)
  })

  it('does not style a caret CodeMirror was never asked to draw', () => {
    // The invariant that would have saved two releases: styling `.cm-cursor` is only meaningful if the extension that
    // creates those elements is installed. Both halves are checked, so neither can quietly arrive alone.
    expect(editor, 'the window installs no drawn caret').not.toMatch(/import\s*\{[^}]*drawSelection/)
    // Rules, not prose: a comment is allowed to name the element while explaining why nothing styles it. Checked on
    // both places a selector can live — a `:global(...)` in the stylesheet and a key in the theme object.
    const globalSelectors = [...editor.matchAll(/:global\(([^)]*)\)/g)].map((m) => m[1]).join('\n')
    expect(globalSelectors, 'a stylesheet rule for an element that is never created').not.toContain('cm-cursor')
    expect(editor).not.toMatch(/'\.cm-cursor[^']*':/);
  })

  it('keeps the caret visible in Windows high-contrast mode', () => {
    // Forced colours replace custom properties, so `var(--ink)` is exactly what does not survive there. The system's
    // text-on-canvas colour does — and with the native caret this is the only caret rule that can matter.
    expect(editor).toMatch(/forced-colors:\s*active[\s\S]{0,200}caret-color:\s*CanvasText/)
  })

  it('keeps the native carets in the window on the same ruler', () => {
    expect(appCss).toMatch(/caret-color:\s*var\(--ink\)/)
  })

  it('stops naming a CodeMirror variable that CodeMirror does not have', () => {
    // 0.0.16 set `--cm-cursor: var(--ink)` and commented that CodeMirror takes its caret colour from that variable. It
    // does not: the base theme hardcodes `borderLeft: "1.2px solid black"` and `&dark .cm-cursor { borderLeftColor:
    // "#ddd" }` — both of which apply to elements this app never creates. A variable nobody reads is not a fix, and a
    // comment that says otherwise sends the next person to the wrong file.
    expect(editor).not.toContain('--cm-cursor')
    const codeMirror = readFileSync(
      fileURLToPath(new URL('../node_modules/@codemirror/view/dist/index.js', import.meta.url)),
      'utf8',
    )
    expect(codeMirror, 'the claim was checked against the dependency, and the dependency has no such variable').not.toContain(
      '--cm-cursor',
    )
  })
})

describe('the catalogue row still looks like something you do not edit', () => {
  it('styles the class the tree actually emits', () => {
    // These rows used to be `disabled`, which carried the dimming as a side effect of an attribute whose real job was
    // "unclickable". Making np's catalogue readable removed that attribute — and with it the only rule that greyed
    // the row, while three sentences in the docs went on promising one. A class needs a rule; that is the whole test.
    expect(treeSvelte).toContain('class:greyed={row.greyed}')
    const start = treeSvelte.indexOf('.tree button.greyed')
    expect(start, 'the .greyed class has no styling behind it any more').toBeGreaterThan(-1)
    const rule = treeSvelte.slice(start, treeSvelte.indexOf('}', start) + 1)
    expect(rule.toLowerCase()).toContain('italic')
    // The dimming may live in the rule or behind a token — what must not happen is a `.greyed` class that resolves to
    // nothing. So follow the token: if the row borrows `--faint`, `app.css` has to define that as an ink mix, or the
    // catalogue rows come back full-strength and this test says so here instead of in somebody's screenshot.
    const viaToken = /color:\s*var\(--faint\)/.test(rule)
    if (viaToken) {
      expect(appCss, '--faint is used to grey a row but is not defined as a mix').toMatch(
        /--faint:\s*color-mix\(in srgb,\s*var\(--ink\)\s+\d+%,\s*transparent\)/
      )
    } else {
      expect(rule).toContain('color-mix')
    }
  })

  it('does not light up under the pointer like an ordinary note', () => {
    // Hover says "this will do something". np's catalogue row opens read-only, which is worth showing, but it should
    // not be dressed exactly like a row that will take your typing.
    expect(treeSvelte).toMatch(/\.tree button:hover:not\(:disabled\):not\(\.greyed\)/)

    // A folder has to look like a folder before it has anything in it. The twisty used to appear only once a folder had
    // children, so a newly made one was bold text with no mark — the tester's "not good differentiable from files", and
    // half of "+ folder → nothing happens". jsdom can read the markup; these rules are the other half.
    expect(treeSvelte, 'an empty folder still gets a place in the twisty column').toMatch(/\.twisty\.faint/)
    expect(treeSvelte, 'and it says so in words').toMatch(/\.empty-hint/)
  })
})

describe('dark mode is the media query, because nobody sets a `.dark` class', () => {
  const appSvelte = readFileSync(fileURLToPath(new URL('./App.svelte', import.meta.url)), 'utf8')
  const statusLine = readFileSync(fileURLToPath(new URL('./lib/StatusLine.svelte', import.meta.url)), 'utf8')
  const editor = readFileSync(fileURLToPath(new URL('./lib/Editor.svelte', import.meta.url)), 'utf8')
  const darkMode = readFileSync(fileURLToPath(new URL('./lib/darkMode.ts', import.meta.url)), 'utf8')

  it('adds no class for the window to wear', () => {
    // Three stylesheets used to hang their dark colours off `:global(.dark)` — and nothing in the app ever added that
    // class. `darkMode.ts` reads `prefers-color-scheme` and tells CodeMirror its facet; that is the entire mechanism,
    // so a class-based rule was an amber nobody on a dark desktop ever saw.
    for (const [name, src] of Object.entries({ appSvelte, statusLine, editor, darkMode, treeSvelte })) {
      expect(src, `${name} writes a .dark selector that nothing can produce`).not.toMatch(/class:dark=|classList\.(add|toggle)\(['"]dark/)
    }
  })

  it('paints the brighter amber where the media query can actually reach it', () => {
    for (const [name, src] of Object.entries({ appSvelte, statusLine, editor })) {
      expect(src, `${name} still styles a class no element wears`).not.toContain(':global(.dark)')
      expect(src, `${name}'s dark amber is not inside a prefers-color-scheme block`).toMatch(
        /@media \(prefers-color-scheme: dark\)[\s\S]{0,220}#d9a441/
      )
    }
  })
})

describe('the marker colours and the states that produce them', () => {
  /** The union in `tree.ts`, read from its own declaration. */
  const tones = (treeTs.match(/export type MarkerTone = ([^;]+);/)?.[1] ?? '').match(/'[a-z]+'/g) ?? []

  it('declares the tones where the decision is made', () => {
    expect(tones.length, 'no MarkerTone union in tree.ts').toBeGreaterThan(1)
    expect(tones.map((t) => t.replaceAll("'", ''))).toEqual(
      expect.arrayContaining(['embedded', 'queued', 'waiting', 'refused', 'none'])
    )
  })

  it('draws exactly those tones, and no others', () => {
    // Twice now this project has shipped a style that could never apply: a `.greyed` class whose rule had been
    // deleted, and `--cm-cursor`, which CodeMirror does not have. Same shape of bug in the other direction — CSS
    // inventing a tone no row can have — so both directions are checked here rather than in a screenshot.
    const declared = tones.map((t) => t.replaceAll("'", ''))
    for (const tone of declared.filter((t) => t !== 'none')) {
      expect(treeSvelte, `nothing colours a ${tone} marker`).toContain(`.marker[data-tone='${tone}']`)
    }
    const styled = [...treeSvelte.matchAll(/\.marker\[data-tone='([a-z]+)'\]/g)].map((m) => m[1])
    for (const tone of new Set(styled)) {
      expect(declared, `Tree.svelte styles '${tone}', which no state produces`).toContain(tone)
    }
  })

  it('shows the key with a word per colour, built from the same sentences as the tooltips', () => {
    expect(treeSvelte).toContain('data-testid="marker-legend"')
    expect(treeSvelte, 'the key must come from MARKER_LEGEND, not from prose typed into markup').toContain('{#each MARKER_LEGEND')
  })

  it('keeps a shape for every colour, because colour alone carries nothing', () => {
    // The glyph is hidden behind the disc and shown again where hues go away.
    expect(treeSvelte).toMatch(/\.marker \{[^}]*\}/s)
    expect(treeSvelte).toMatch(/@media \(forced-colors: active\)[\s\S]{0,200}\.marker \.glyph \{\s*display: inline/)
  })
})
