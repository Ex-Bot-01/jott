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

  it('hands the caret colour to CodeMirror as a theme, not only as CSS', () => {
    // The rule above wins today by being more specific than what CodeMirror injects at runtime. Winning a race is
    // not the same as not racing: the same light facet also paints a bright blue selection and a `black` native
    // caret, and any future CM selector can arrive later and heavier. Through `EditorView.theme` the colours land
    // inside CodeMirror's own style module, in the facet the window is actually in.
    const theme = editor.slice(editor.indexOf('EditorView.theme'))
    expect(theme, 'the editor builds no CodeMirror theme').not.toBe('')
    expect(theme).toMatch(/borderLeft:\s*['"]2px solid var\(--ink\)['"]/)
    // The facet itself: without it, CM's `&dark` variants never apply and the light ones contradict the palette.
    expect(editor).toContain('EditorView.darkTheme.of')
  })

  it('stops the caret from switching itself off mid-blink', () => {
    // CodeMirror blinks with `steps(1)` to `opacity: 0`, so the caret is genuinely absent half the time. "Hard to
    // see where the cursor is" is a fair description of that, in either scheme.
    expect(editor).toMatch(/@keyframes\s+jott-caret[\s\S]*?opacity:\s*0?\.[1-9]/)
    expect(editor).toContain('animation-name: jott-caret')
    // …and stands still for someone who asked their OS for no motion.
    expect(editor).toMatch(/prefers-reduced-motion:\s*reduce[\s\S]*?animation:\s*none/)
  })

  it('keeps the caret visible in Windows high-contrast mode', () => {
    // Forced colours replace custom properties, so `var(--ink)` is exactly what does not survive there. The system
    // colour for text on the canvas does.
    expect(editor).toMatch(/forced-colors:\s*active[\s\S]*?border-left-color:\s*CanvasText/)
  })

  it('keeps the native carets in the window on the same ruler', () => {
    expect(appCss).toMatch(/caret-color:\s*var\(--ink\)/)
  })

  it('draws a caret even when CodeMirror does not believe it is focused', () => {
    // Read out of `@codemirror/view/dist/index.js`, the two rules that decide whether a caret exists at all:
    //
    //     ".cm-cursor": { display: "none" }
    //     "&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor": { display: "block" }
    //
    // Colour and width are irrelevant to a `display: none` element. `cm-focused` is CodeMirror's own belief about
    // focus, derived from its observers — so the window tracks DOM focus itself and shows the caret on that as well.
    // This is the third release at this bug; the two earlier ones coloured and widened a caret that was never drawn.
    // `[^{]*` rather than a fixed shape: the rule is a selector list (caret and drop caret), so the brace is several
    // names away from the class that turns it on.
    expect(editor).toMatch(/\.jott-caret-focus[^{]*\.cm-cursor[^{]*\{[^}]*display:\s*block/)
  })

  it('falls back to the native caret only while CodeMirror thinks it is unfocused', () => {
    // CodeMirror hides the real caret with `caretColor: "transparent !important"` on `.cm-content`, which is why
    // v0.0.17's `caret-color: var(--ink)` in app.css could not do anything: an `!important` needs an `!important`
    // and a heavier selector to answer. Un-hiding it unconditionally would put two carets on screen whenever
    // CodeMirror's own layer is working, so the fallback waits for exactly the case where it is not.
    expect(editor).toMatch(/:not\(\.cm-focused\)[\s\S]{0,120}caret-color:\s*var\(--ink\)\s*!important/)
  })

  it('refuses a caret that was measured zero pixels tall', () => {
    // Each `.cm-cursor` gets its height from CodeMirror's line measurement, inline. A measurement of 0 — before a
    // font loads, or on a webview that reports an empty client rect — is an invisible caret with no explanation.
    expect(editor).toMatch(/\.cm-cursor[\s\S]{0,200}min-height:\s*1em/)
  })

  it('stops naming a CodeMirror variable that CodeMirror does not have', () => {
    // v0.0.16 set `--cm-cursor` on the editor and commented that CodeMirror takes its caret colour from it. It does
    // not: the base theme hardcodes `borderLeft: "1.2px solid black"` and `&dark .cm-cursor { borderLeftColor: "#ddd" }`.
    // A variable nobody reads is not a fix, and a comment that says otherwise sends the next person to the wrong file.
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

/** A row that is not yours has to look different, in a way a test can see. */
describe('the catalogue row still looks like something you do not edit', () => {
  it('styles the class the tree actually emits', () => {
    // These rows used to be `disabled`, which carried the dimming as a side effect of an attribute whose real job was
    // "unclickable". Making np's catalogue readable removed that attribute — and with it the only rule that greyed
    // the row, while three sentences in the docs went on promising one. A class needs a rule; that is the whole test.
    expect(treeSvelte).toContain('class:greyed={row.greyed}')
    const start = treeSvelte.indexOf('.tree button.greyed')
    expect(start, 'the .greyed class has no styling behind it any more').toBeGreaterThan(-1)
    const rule = treeSvelte.slice(start, treeSvelte.indexOf('}', start) + 1)
    expect(rule).toContain('color-mix')
    expect(rule.toLowerCase()).toContain('italic')
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
