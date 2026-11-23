// What the "Versions & sync" section and the versions panel say, as pure functions of what core answered. The
// components draw; the sentences live here, where they have tests.

import type { GitReport, HistoryStatus, SyncReport } from './ipc'
import type { SavePhase } from './saveState'

/** Everything the section can ask for, through one callback. */
export type HistoryAction =
  | { kind: 'checkGit' }
  | { kind: 'setGitPath'; path: string | null }
  | { kind: 'enable' }
  | { kind: 'disable' }
  | { kind: 'identity'; name: string; email: string }
  | { kind: 'options'; quietSecs: number; syncOnOpenClose: boolean }
  | { kind: 'versionNow' }
  | { kind: 'connect'; url: string }
  | { kind: 'disconnect' }
  | { kind: 'sync' }

export const QUIET_CHOICES: { secs: number; label: string }[] = [
  { secs: 30, label: 'after 30 seconds without changes' },
  { secs: 60, label: 'after a minute without changes' },
  { secs: 300, label: 'after 5 minutes without changes' },
  { secs: 900, label: 'after 15 minutes without changes' },
]

const SOURCE_WORDS: Record<NonNullable<GitReport['source']>, string> = {
  configured: 'chosen in Settings',
  path: 'found on the program path',
  usual: 'found where git installs itself',
}

/** One line about the git in use, or why there is none. */
export function gitLine(git: GitReport): string {
  if (!git.found) return git.problem ?? 'git is not installed on this computer, or Jott cannot find it'
  const how = git.source ? `, ${SOURCE_WORDS[git.source]}` : ''
  return `Using git ${git.version ?? ''} (${git.path ?? 'unknown path'}${how}).`
}

/** The offer shown where there is no git: what it would add, and where to get it. */
export const GIT_OFFER =
  'Jott can keep versions of your notes, restore an earlier one, and back them up to GitHub, GitLab or Gitea — ' +
  'using git. Install git, then press “Check again”: Jott finds it without a restart.'

/** What turning history on will do here, said before the button is pressed. */
export function enableSentence(status: HistoryStatus): string | null {
  switch (status.existing) {
    case 'inside':
      return `This folder is inside the git repository at ${status.insideTop ?? 'a folder above it'}. Jott keeps versions only for a folder that is its own repository, so its saves never land in a project somebody else keeps.`
    case 'atRoot':
      return status.cloud
        ? `This folder already uses git. ${status.cloud} syncs it too, and a repository synced file by file can be damaged when two computers write to it — keep that in mind before using it.`
        : 'This folder already uses git. Jott can save versions into that history.'
    case 'appData':
      return 'Jott kept versions of this folder before; turning them on again continues that history.'
    case 'nothing':
      return status.cloud
        ? `${status.cloud} syncs this folder, so Jott keeps its history in its own data folder, out of reach of the sync. Versions stay on this computer.`
        : 'Jott makes this folder a git repository and saves a version after you stop typing for a while.'
    default:
      return null
  }
}

export function enableLabel(status: HistoryStatus): string {
  return status.existing === 'atRoot' ? 'Use existing history' : 'Turn on version history'
}

/** `1536` → `1.5 KB`; sizes people compare against a 50 MB limit. */
export function bytesText(bytes: number): string {
  const units = ['bytes', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return unit === 0 ? `${value} bytes` : `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`
}

/** `just now`, `4 minutes ago`, `3 hours ago`, or the date. */
export function whenText(atMs: number, nowMs: number): string {
  const seconds = Math.max(0, Math.round((nowMs - atMs) / 1000))
  if (seconds < 45) return 'just now'
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) return minutes === 1 ? 'a minute ago' : `${minutes} minutes ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return hours === 1 ? 'an hour ago' : `${hours} hours ago`
  return new Date(atMs).toLocaleString()
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

/** What the last sync did, in one line. */
export function syncSentence(report: SyncReport | null, nowMs: number): string | null {
  if (!report) return null
  const when = whenText(report.atMs, nowMs)
  if (report.error) return `Sync ${when} did not finish: ${report.error}`
  const outcome = report.outcome
  if (!outcome) return null
  switch (outcome.kind) {
    case 'upToDate':
      return `Synced ${when}: already up to date.`
    case 'pushed':
      return `Synced ${when}: sent ${plural(outcome.commits, 'version', 'versions')}.`
    case 'pulled':
      return `Synced ${when}: received ${plural(outcome.commits, 'version', 'versions')}, ${plural(outcome.paths.length, 'file', 'files')} changed.`
    case 'merged': {
      const base = `Synced ${when}: received ${plural(outcome.pulled, 'version', 'versions')} and sent ${plural(outcome.pushed, 'version', 'versions')}.`
      if (outcome.keptBoth.length === 0) return base
      return `${base} ${plural(outcome.keptBoth.length, 'note was', 'notes were')} changed on both sides — both texts are kept.`
    }
  }
}

/** The versions line under the toggle. */
export function versionLine(status: HistoryStatus, nowMs: number): string {
  if (status.problem) return status.problem
  const last = status.lastVersionAtMs === null ? 'No version saved this session yet.' : `Last version saved ${whenText(status.lastVersionAtMs, nowMs)}.`
  return status.pending ? `${last} Changes since then are saved once you pause.` : last
}

/** A restore writes the note, so an unsaved buffer would be overwritten: only a buffer that matches the disk may. */
export function canRestore(phase: SavePhase): boolean {
  return phase === 'clean' || phase === 'saved'
}

/** An address with a login in it, refused here too so the secret never crosses the wire. Core refuses it as well. */
export function hasLogin(url: string): boolean {
  const match = /^[a-z][a-z0-9+.-]*:\/\/([^/]*)/i.exec(url.trim())
  if (!match) return false
  const at = match[1].lastIndexOf('@')
  if (at < 0) return false
  return /^https?:/i.test(url.trim()) || match[1].slice(0, at).includes(':')
}

export type DiffLine = { kind: 'same' | 'added' | 'removed'; text: string }

/** Lines that differ between a version and the note now. `removed` lines are in the version only — a restore brings
 *  them back; `added` lines are in the note now only. Above a size a person would scroll through anyway, the two are
 *  shown whole rather than spending seconds on an exact match. */
export function lineDiff(version: string, now: string, limit = 4000): DiffLine[] {
  const a = version.split('\n')
  const b = now.split('\n')
  if (a.length * b.length > limit * limit) {
    return [...a.map((text) => ({ kind: 'removed' as const, text })), ...b.map((text) => ({ kind: 'added' as const, text }))]
  }
  const rows = a.length + 1
  const cols = b.length + 1
  const lcs = new Uint32Array(rows * cols)
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      lcs[i * cols + j] = a[i] === b[j] ? lcs[(i + 1) * cols + j + 1] + 1 : Math.max(lcs[(i + 1) * cols + j], lcs[i * cols + j + 1])
    }
  }
  const out: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: 'same', text: a[i] })
      i += 1
      j += 1
    } else if (lcs[(i + 1) * cols + j] >= lcs[i * cols + j + 1]) {
      out.push({ kind: 'removed', text: a[i] })
      i += 1
    } else {
      out.push({ kind: 'added', text: b[j] })
      j += 1
    }
  }
  while (i < a.length) out.push({ kind: 'removed', text: a[i++] })
  while (j < b.length) out.push({ kind: 'added', text: b[j++] })
  return out
}
