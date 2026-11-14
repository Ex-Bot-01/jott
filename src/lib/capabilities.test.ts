// The shell's capability set is a runtime contract with no compiler behind it. A permission dropped from this JSON
// file fails nothing at build time: the webview call simply rejects, and every caller here swallows that on purpose
// (a refused notification should never break a save). So the grants the window actually depends on are asserted here,
// where deleting one is a red test rather than a silent no-op on somebody's desktop.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const capabilities = JSON.parse(
  readFileSync(fileURLToPath(new URL('../../apps/shell/capabilities/default.json', import.meta.url)), 'utf8'),
) as { permissions: string[]; windows: string[] }

describe('what the shell is allowed to do', () => {
  it.each([
    ['opener:default', 'external links and "Show log file" (revealItemInDir) stop working'],
    ['notification:default', 'the model-ready line never reaches somebody who alt-tabbed away'],
    ['dialog:default', '"Set up embeddings" cannot ask its question'],
  ])('grants %s, or %s', (permission, consequence) => {
    expect(capabilities.permissions, consequence).toContain(permission)
  })

  it('asks for one window and no more', () => {
    // The capability set is scoped to `main`; a second entry here would be a second webview with the same grants.
    expect(capabilities.windows).toEqual(['main'])
  })

  it('does not reach for a permission the window does not use', () => {
    // `allow-open-path` is what "Open log folder" used to call, and what the refusal of turned into a box of copied
    // text (0.0.13). Revealing the file needs no extra grant — if this ever fails because somebody added it back,
    // the fix is to remove the call, not to widen the set.
    expect(capabilities.permissions.join(' ')).not.toContain('opener:allow-open-path')
  })
})
