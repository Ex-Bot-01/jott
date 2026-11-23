import { describe, expect, it } from 'vitest'
import type { GitReport, HistoryStatus } from './ipc'
import { bytesText, canRestore, enableLabel, enableSentence, gitLine, hasLogin, lineDiff, syncSentence, versionLine, whenText } from './history'

const install = { url: 'https://git-scm.com/downloads', commands: ['winget install --id Git.Git -e --source winget'] }

function git(over: Partial<GitReport> = {}): GitReport {
  return { found: true, path: 'C:\\Program Files\\Git\\cmd\\git.exe', version: '2.56.0.windows.2', source: 'usual', configured: null, problem: null, install, ...over }
}

function status(over: Partial<HistoryStatus> = {}): HistoryStatus {
  return {
    git: git(),
    enabled: false,
    existing: 'nothing',
    insideTop: null,
    layout: null,
    cloud: null,
    identity: null,
    remote: null,
    syncOffered: true,
    quietSecs: 60,
    syncOnOpenClose: false,
    tooLarge: [],
    lastVersionAtMs: null,
    pending: false,
    lastSync: null,
    problem: null,
    ...over,
  }
}

const NOW = 1_800_000_000_000

describe('the git line', () => {
  it('says which git and how it was found', () => {
    expect(gitLine(git())).toBe('Using git 2.56.0.windows.2 (C:\\Program Files\\Git\\cmd\\git.exe, found where git installs itself).')
  })

  it('carries core’s sentence when there is no git', () => {
    expect(gitLine(git({ found: false, problem: 'git 2.25.1 at /usr/bin/git is too old — Jott needs 2.28.0 or newer' }))).toContain('too old')
  })
})

describe('turning history on', () => {
  it('says before the press what will happen in a cloud-synced folder', () => {
    expect(enableSentence(status({ cloud: 'OneDrive' }))).toContain('OneDrive syncs this folder, so Jott keeps its history in its own data folder')
  })

  it('names the repository a folder sits inside, and offers to adopt one at its root', () => {
    expect(enableSentence(status({ existing: 'inside', insideTop: 'D:\\project' }))).toContain('D:\\project')
    expect(enableLabel(status({ existing: 'atRoot' }))).toBe('Use existing history')
    expect(enableSentence(status({ existing: 'atRoot', cloud: 'Dropbox' }))).toContain('Dropbox syncs it too')
  })
})

describe('sentences about time, size and sync', () => {
  it('reads like a person', () => {
    expect(whenText(NOW - 10_000, NOW)).toBe('just now')
    expect(whenText(NOW - 4 * 60_000, NOW)).toBe('4 minutes ago')
    expect(bytesText(120 * 1024 * 1024)).toBe('120 MB')
    expect(bytesText(512)).toBe('512 bytes')
  })

  it('says when both texts were kept', () => {
    const sentence = syncSentence(
      { atMs: NOW, error: null, outcome: { kind: 'merged', pulled: 1, pushed: 2, paths: ['a.md'], keptBoth: [{ rel: 'a.md', remoteCopy: 'a (from github.com, 2026-11-23).md' }] } },
      NOW,
    )
    expect(sentence).toBe('Synced just now: received 1 version and sent 2 versions. 1 note was changed on both sides — both texts are kept.')
  })

  it('shows git’s own sentence when a sync failed', () => {
    expect(syncSentence({ atMs: NOW, error: 'git push failed: Authentication failed', outcome: null }, NOW)).toContain('Authentication failed')
  })

  it('puts a problem ahead of the version line', () => {
    expect(versionLine(status({ problem: 'git does not know who you are yet' }), NOW)).toBe('git does not know who you are yet')
    expect(versionLine(status({ lastVersionAtMs: NOW - 120_000, pending: true }), NOW)).toBe(
      'Last version saved 2 minutes ago. Changes since then are saved once you pause.',
    )
  })
})

describe('restoring', () => {
  it('is only offered while nothing typed would be overwritten', () => {
    expect(canRestore('clean')).toBe(true)
    expect(canRestore('saved')).toBe(true)
    expect(canRestore('dirty')).toBe(false)
    expect(canRestore('conflict')).toBe(false)
  })

  it('shows what a restore brings back and what it removes', () => {
    expect(lineDiff('a\nold\nc', 'a\nnew\nc')).toEqual([
      { kind: 'same', text: 'a' },
      { kind: 'removed', text: 'old' },
      { kind: 'added', text: 'new' },
      { kind: 'same', text: 'c' },
    ])
  })

  it('gives up on an exact match for huge notes rather than freezing', () => {
    const big = Array.from({ length: 50 }, (_, i) => `line ${i}`).join('\n')
    const diff = lineDiff(big, big, 10)
    expect(diff.filter((l) => l.kind === 'removed')).toHaveLength(50)
  })
})

describe('remote addresses', () => {
  it('spots a login before it is sent', () => {
    expect(hasLogin('https://ghp_token@github.com/a/b.git')).toBe(true)
    expect(hasLogin('ssh://u:pw@host/a')).toBe(true)
    expect(hasLogin('git@github.com:a/b.git')).toBe(false)
    expect(hasLogin('ssh://git@host/a')).toBe(false)
    expect(hasLogin('https://gitea.local/a/b')).toBe(false)
  })
})
