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
