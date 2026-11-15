// The wire between Rust and this file has no compiler across it. `ipc_surface.rs` pins what Rust sends; this file
// reads that same source and checks the TypeScript unions still name all of it. A kind added on one side and not
// the other does not fail a build — it shows up as a branch nobody took and a sentence nobody wrote.
/// <reference types="node" />
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const ipcRust = readFileSync(fileURLToPath(new URL('../../crates/jott-core/src/ipc.rs', import.meta.url)), 'utf8')

/** The variants of a `#[serde(rename_all = "camelCase")]` enum, as the wire spells them. */
function camelVariants(source: string, enumName: string): string[] {
  const start = source.indexOf(`pub enum ${enumName} {`)
  expect(start, `${enumName} is not in ipc.rs any more — update this test rather than deleting it`).toBeGreaterThan(-1)
  const body = source.slice(start, source.indexOf('}', start))
  // A variant is a line that starts with an identifier and no indentation continuation.
  return [...body.matchAll(/^\s{4}([A-Z][A-Za-z0-9]*)[,;]?$/gm)].map((m) => m[1][0].toLowerCase() + m[1].slice(1))
}

const ipcTs = readFileSync(fileURLToPath(new URL('./ipc.ts', import.meta.url)), 'utf8')

function kindUnion(source: string): string[] {
  const block = source.slice(source.indexOf('export interface IpcError'), source.indexOf('}', source.indexOf('export interface IpcError')))
  return [...block.matchAll(/'([A-Za-z0-9]+)'/g)].map((m) => m[1]).filter((k) => k !== 'kind')
}

describe('the error vocabulary', () => {
  it('names every kind Rust can send', () => {
    expect(kindUnion(ipcTs).sort()).toEqual(camelVariants(ipcRust, 'ErrorKind').sort())
  })

  it('keeps the two np kinds apart, because they need different sentences', () => {
    const kinds = kindUnion(ipcTs)
    expect(kinds).toContain('npGone')
    expect(kinds).toContain('npRefused')
    expect(kinds).toContain('npTooOld')
  })

  it('reads the file it claims to read', () => {
    // Without this, a renamed enum would make the comparison above vacuous: zero kinds against zero kinds.
    expect(ipcRust).toContain('pub enum ErrorKind')
    expect(camelVariants(ipcRust, 'ErrorKind').length).toBeGreaterThan(5)
  })
})

/** The field names of a `#[serde(rename_all = "camelCase")]` struct, spelled the way the wire spells them. */
function rustFields(source: string, structName: string): string[] {
  const start = source.indexOf(`pub struct ${structName} {`)
  expect(start, `${structName} is not in ipc.rs any more — update this test rather than deleting it`).toBeGreaterThan(-1)
  const body = source.slice(start, source.indexOf('\n}', start))
  const fields = [...body.matchAll(/^\s{4}(?:pub )?[a-z_][a-z0-9_]*:/gm)].map((m) => {
    const name = m[0].trim().replace(/^pub /, '').replace(/:$/, '')
    return name.replace(/_([a-z0-9])/g, (_all, c: string) => c.toUpperCase())
  })
  // The same guard as above: a struct whose fields the regex missed would compare empty against empty.
  expect(fields.length, `${structName} has no readable fields`).toBeGreaterThan(1)
  return fields
}

function interfaceKeys(source: string, name: string): string[] {
  const start = source.indexOf(`export interface ${name} {`)
  expect(start, `${name} is not in ipc.ts any more`).toBeGreaterThan(-1)
  const body = source.slice(start, source.indexOf('\n}', start))
  return [...body.matchAll(/^\s{2}([A-Za-z0-9]+)\??:/gm)].map((m) => m[1])
}

describe('the search answer', () => {
  it('carries every key Rust sends, including what np said about the run', () => {
    // `indexed`, `keywordUsed`, `semanticUsed` and `notice` are the difference between "Nothing matched" and "this
    // folder has never been indexed". A field dropped on this side silently becomes `undefined`, and `undefined` is
    // what an older np sends — so the window would lose the fact and keep the doubt.
    expect(interfaceKeys(ipcTs, 'SearchResults').sort()).toEqual(rustFields(ipcRust, 'SearchResults').sort())
  })

  it('carries every key Rust sends about one hit', () => {
    // `matchedExactly` is the difference between a row that matched what somebody typed and a row np reached by a word
    // beginning. Dropped on this side it becomes `undefined`, which renders exactly like a hit whose exactness nobody
    // knows — so the widening would go unmarked on screen while every other test still passed.
    expect(interfaceKeys(ipcTs, 'SearchHit').sort()).toEqual(rustFields(ipcRust, 'HitDto').sort())
  })
})

describe('what a note carries', () => {
  it('carries every key Rust sends about the file that is open', () => {
    // `readonly` is the difference between an editor and a display: without it the window offers a buffer for
    // note-processor's own catalogue, and the save fails after somebody has typed into it.
    expect(interfaceKeys(ipcTs, 'NoteContent').sort()).toEqual(rustFields(ipcRust, 'NoteContent').sort())
  })
})

describe('the consent receipt', () => {
  it('carries every key Rust sends about what note-processor did', () => {
    // Since np 0.5.x the answer comes back over the pipe (`embeddings.enable`), and the card quotes it. A key dropped
    // on this side turns "Jott could not write models.auto_fetch" into `undefined`, which renders as silence about the
    // one thing the person needs to know: whether the download guardrail actually moved.
    expect(interfaceKeys(ipcTs, 'ConsentOutcome').sort()).toEqual(rustFields(ipcRust, 'ConsentAnswerDto').sort())
  })
})

describe('the consent card payload', () => {
  it('carries every key Rust sends about np’s configuration', () => {
    // The card is the only thing standing between "Allow download" and a person editing np's config by guesswork.
    // A dropped key would quietly turn "np says auto_fetch is false" into `undefined`, and the card would say
    // nothing about the one setting that decides whether anything gets fetched.
    expect(interfaceKeys(ipcTs, 'ConsentHelp').sort()).toEqual(rustFields(ipcRust, 'ConsentHelp').sort())
  })

  it('names every gap Rust can report, on both sides of the wire', () => {
    // Both halves used to be checked against themselves: this file asserted that ipc.ts mentions the words, and
    // `ipc_surface.rs` asserted that Rust produces them. Renaming one side's spelling passed both suites and sent
    // an unknown gap to a card that then gave advice about a provider somebody chose on purpose.
    const block = ipcTs.slice(ipcTs.indexOf('export interface ConsentHelp'), ipcTs.indexOf('}', ipcTs.indexOf('export interface ConsentHelp')))
    for (const gap of ['providerMissing', 'modelMissing', 'providerOther', 'configUnreadable', 'ready']) {
      expect(block, `gap ${gap} is missing from the TypeScript union`).toContain(`'${gap}'`)
      // `=> "word"`, not `"word"`: the enum mapping is the contract, and a doc comment that still spells the old word
      // must not be allowed to satisfy a pin on its own.
      expect(ipcRust, `core no longer answers "${gap}" — rename both sides or neither`).toContain(`=> "${gap}"`)
    }
  })
})
