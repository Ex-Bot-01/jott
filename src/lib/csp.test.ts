// The shipped window used to run with `"csp": null` — no document policy at all, on a page that holds the whole
// vault behind `window.__TAURI_INTERNALS__.invoke`. Nothing in `src/` renders a stranger's markup (there is no
// `{@html}` anywhere), so today there is nothing to inject; this file is about keeping it that way, because two
// decisions make the window more open than it looks:
//
// * the bridge is on the page object, not bundled away — any script that ever runs in this document can call every
//   command in `generate_handler!`, including `set_consent("granted")` (a ~360 MB download) and `openVault(path)`
//   (an arbitrary path);
// * devtools are compiled into release builds (`tauri = { features = ["devtools"] }`, pinned by
//   `crates/jott-core/tests/shell_window.rs`), so a person can paste code into this document, and a future
//   dependency can too.
//
// Tauri injects the policy at build time — as a `<meta>` in the HTML on Linux and as a response header on Windows
// and macOS (`tauri-2.12.5/src/manager/mod.rs:53`, `set_csp`) — so nothing here can be *run* on this machine, and
// dev is unaffected: with `devUrl` set the page comes from Vite and Tauri never sees it. What these pins hold is
// the string, and the two facts inside it that are easy to get wrong.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const config = JSON.parse(readFileSync(fileURLToPath(new URL('../../apps/shell/tauri.conf.json', import.meta.url)), 'utf8')) as {
  app: { security?: { csp?: string | null } }
}
const page = readFileSync(fileURLToPath(new URL('../../index.html', import.meta.url)), 'utf8')

/** The policy as one string, or the hole this file exists to close. */
const csp = config.app.security?.csp

describe('the document policy the shipped window runs under', () => {
  it('names a policy instead of shipping null', () => {
    expect(csp, '"csp": null means the page loads with no default-src and no frame-ancestors').toBeTypeOf('string')
    expect((csp ?? '').trim().length).toBeGreaterThan(0)
  })

  it('runs only its own scripts', () => {
    // Vite emits one module script and nothing inline (`index.html`'s own `<script>` is `src=`), so `'self'` costs
    // nothing today. Tauri appends hashes for any inline script a later build adds (`set_csp` → `csp_hashes`), so
    // this does not have to be widened the day someone adds one.
    expect(csp ?? '').toContain("script-src 'self'")
  })

  it('allows inline styles, because CodeMirror writes its own and a strict policy fails quietly', () => {
    // The nasty one: `style-src 'self'` does not break the editor loudly. It loads *unstyled* — no error dialog,
    // just a window that looks broken — so it has to be pinned in a test rather than discovered by a tester.
    expect(csp ?? '').toMatch(/style-src[^;]*'unsafe-inline'/)
  })

  it('lets the bridge reach note-processor over both spellings of its origin', () => {
    // `tauri-2.12.5/scripts/ipc-protocol.js:37` sends every command as a `fetch` to the IPC custom protocol, and
    // falls back to `window.ipc.postMessage` only after that fetch fails. Both spellings are needed because a
    // custom protocol is `ipc://localhost/<cmd>` on Linux and macOS but `http://ipc.localhost/<cmd>` on Windows
    // (`tauri-2.12.5/src/protocol/mod.rs:30`, `src/ipc/protocol.rs:611`). Without them every call takes the
    // fallback path and the console fills with "IPC custom protocol failed".
    const connect = (csp ?? '').match(/connect-src([^;]*)/)?.[1] ?? ''
    expect(connect, 'the page still has to reach its own assets').toContain("'self'")
    expect(connect, 'ipc://localhost — Linux and macOS').toContain('ipc:')
    expect(connect, 'http://ipc.localhost — the Windows spelling').toContain('http://ipc.localhost')
  })

  it('refuses to be framed, and refuses a rewritten base href', () => {
    // Neither is covered by `default-src`, which is why both are spelled out. A `<base>` is the quiet one: with no
    // `base-uri`, one injected element repoints every relative asset load at somewhere else.
    expect(csp ?? '').toContain("frame-ancestors 'none'")
    expect(csp ?? '').toContain("base-uri 'none'")
  })

  it('draws images only from its own bundle', () => {
    // Jott renders no image of anybody's: the tree is text, note text goes into CodeMirror, and an image opens
    // outside in whatever the opener plugin launches. If that ever changes — a preview pane, an inline attachment —
    // this pin is the reminder that `asset:` or `data:` has to be added deliberately, with the reason next to it.
    expect(csp ?? '').toMatch(/img-src[^;]*/)
    expect((csp ?? '').match(/img-src([^;]*)/)![1].trim()).toBe("'self'")
  })

  it('falls back to default-src for anything not named', () => {
    expect(csp ?? '').toContain("default-src 'self'")
  })
})

describe('the page the policy is applied to', () => {
  it('loads nothing from outside its own bundle', () => {
    // A remote `<script src>` or `<link href>` would need the policy widened, and would be the beginning of a
    // supply chain in an app that has no reason to have one. Checked as text because `index.html` is also the boot
    // skeleton, and this is about what the file asks for over the network.
    const remote = [...page.matchAll(/(?:src|href)="(https?:)?\/\/[^"]+"/g)].map((m) => m[0])
    expect(remote, `the shipped page must fetch nothing it does not ship: ${remote.join(' ')}`).toEqual([])
  })

  it('keeps its boot skeleton inline rather than remote', () => {
    // The `<style>` in `index.html` is what paints before any script runs — the white-window case. It is inline on
    // purpose, and Tauri hashes it into `script-src`/`style-src` at build time (`set_csp`).
    expect(page).toContain('<style>')
  })
})
