import { describe, expect, it } from 'vitest'
import type { RerankerStatus } from './ipc'
import { manualSteps, rerankerSummary, topNAccepted, topNValue } from './rerankStatus'

function status(over: Partial<RerankerStatus> = {}): RerankerStatus {
  return {
    supported: true,
    npVersion: '0.7.0',
    modelId: 'bge-reranker-v2-m3',
    present: false,
    state: 'absent',
    downloading: null,
    busy: null,
    error: null,
    manual: {
      url: 'https://huggingface.co/smarttasks/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf',
      fileName: 'bge-reranker-v2-m3-Q8_0.gguf',
      sha256: 'd32a877b7266f5275172458616824ce6728502416672052c2da795da3389201e',
      bytes: 635674432,
    },
    topN: null,
    topNDefault: 5,
    configPath: null,
    ...over,
  }
}

describe('what Settings says about the ranking model', () => {
  it('says an older note-processor cannot rerank, and offers nothing to do about it here', () => {
    const said = rerankerSummary(status({ supported: false, npVersion: '0.6.1' }), true, false)
    expect(said.line).toContain('0.6.1')
    expect(said.line).toContain('0.7.0')
    expect(said.manual).toBe(false)
  })

  it('shows the download with np’s own byte counts while it runs', () => {
    const said = rerankerSummary(
      status({ busy: 'fetching', downloading: { modelId: 'bge-reranker-v2-m3', bytesDone: 104857600, bytesTotal: 635674432 } }),
      true,
      false,
    )
    expect(said.line).toBe('Downloading the ranking model… 100 MB of 606 MB')
  })

  it('quotes np’s failure and puts the by-hand steps and a retry on screen', () => {
    const error = 'could not open main for bge-reranker-v2-m3: could not resolve the configured host'
    const said = rerankerSummary(status({ error }), true, false)
    expect(said.line).toContain(error)
    expect(said.line).toContain('your browser can')
    expect(said).toMatchObject({ manual: true, retry: true })
    expect(manualSteps(status()).join(' ')).toContain('bge-reranker-v2-m3-Q8_0.gguf')
  })

  it('says ready once np finds the file at its pinned size', () => {
    expect(rerankerSummary(status({ present: true, state: 'present, size matches the pin' }), true, false).line).toBe(
      'Ranking model ready.',
    )
  })

  it('does not push a download on someone who switched reranking off', () => {
    const said = rerankerSummary(status(), false, false)
    expect(said.manual).toBe(false)
    expect(said.line).toContain('switched on')
  })

  it('says an install is being checked while it runs', () => {
    expect(rerankerSummary(status(), true, true).line).toContain('checking it')
  })
})

describe('the Advanced number', () => {
  it('shows np’s own value, or np’s default when its config names none', () => {
    expect(topNValue(status())).toBe(5)
    expect(topNValue(status({ topN: 12 }))).toBe(12)
    expect(topNValue(null)).toBe(5)
  })

  it('accepts exactly what np accepts, 1 to 64 whole results', () => {
    for (const ok of [1, 5, 64]) expect(topNAccepted(ok)).toBe(true)
    for (const bad of [0, 65, -1, 2.5, Number.NaN]) expect(topNAccepted(bad)).toBe(false)
  })
})
