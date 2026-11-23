<script lang="ts">
  // "Versions & sync": the git Jott found (or where to get one), turning history on, the remote, and the sync.
  // Every rule and sentence is in `history.ts`; this file draws what core answered and hands each press to one
  // callback, so Settings does not grow ten more props.

  import { openUrl } from '@tauri-apps/plugin-opener'
  import type { HistoryStatus } from './ipc'
  import {
    GIT_OFFER,
    QUIET_CHOICES,
    bytesText,
    enableLabel,
    enableSentence,
    gitLine,
    hasLogin,
    syncSentence,
    versionLine,
    type HistoryAction,
  } from './history'

  let {
    status,
    vaultOpen,
    busy = false,
    error = null,
    onhistory,
  }: {
    /** `null` while the first answer is on its way. */
    status: HistoryStatus | null
    vaultOpen: boolean
    /** A git call is running; buttons wait rather than queue a second one. */
    busy?: boolean
    /** The last refusal, in core's words. */
    error?: string | null
    onhistory: (action: HistoryAction) => void
  } = $props()

  let name = $state('')
  let email = $state('')
  let url = $state('')
  let gitPath = $state('')
  const now = $derived(Date.now())
  const urlHasLogin = $derived(hasLogin(url))
  const quietKnown = $derived(QUIET_CHOICES.some((c) => c.secs === status?.quietSecs))
</script>

<fieldset data-testid="settings-history">
  <legend>Versions &amp; sync</legend>
  {#if !vaultOpen}
    <p class="quiet">Open a folder to keep versions of its notes.</p>
  {:else if status === null}
    <p class="quiet">Looking for git…</p>
  {:else}
    {#if error}
      <p class="warn" data-testid="history-error">{error}</p>
    {/if}

    {#if !status.git.found}
      <p data-testid="history-offer">{GIT_OFFER}</p>
      <p class="quiet" data-testid="git-problem">{gitLine(status.git)}</p>
      <ul class="lines">
        <li>
          Download: <a href={status.git.install.url} onclick={(e) => { e.preventDefault(); void openUrl(status.git.install.url).catch(() => {}) }}>{status.git.install.url}</a>
        </li>
        {#each status.git.install.commands as command}<li>or run <code>{command}</code></li>{/each}
      </ul>
      <div class="buttons">
        <button type="button" data-testid="git-check" disabled={busy} onclick={() => onhistory({ kind: 'checkGit' })}>Check again</button>
      </div>
    {:else}
      <p class="quiet" data-testid="git-line">{gitLine(status.git)}</p>

      {#if !status.enabled}
        {@const sentence = enableSentence(status)}
        {#if sentence}<p data-testid="history-enable-sentence">{sentence}</p>{/if}
        <div class="buttons">
          <button
            type="button"
            data-testid="history-enable"
            disabled={busy || status.existing === 'inside' || status.existing === 'unknown'}
            onclick={() => onhistory({ kind: 'enable' })}
          >
            {enableLabel(status)}
          </button>
        </div>
      {:else}
        <p data-testid="history-version-line">{versionLine(status, now)}</p>

        {#if status.identity === null}
          <!-- git puts a name on every version; without one it refuses, and Jott does not invent one. Written into this
               folder's repository only. -->
          <form class="identity" onsubmit={(e) => { e.preventDefault(); onhistory({ kind: 'identity', name, email }) }}>
            <span>Name your versions</span>
            <input placeholder="Name" bind:value={name} aria-label="Name for versions" data-testid="history-name" />
            <input placeholder="Email" type="email" bind:value={email} aria-label="Email for versions" data-testid="history-email" />
            <button type="submit" disabled={busy || !name.trim() || !email.includes('@')}>Save</button>
          </form>
        {:else}
          <p class="quiet">Versions are saved as {status.identity.name} &lt;{status.identity.email}&gt;.</p>
        {/if}

        <label class="row">
          <span>Save a version</span>
          <select
            data-testid="history-quiet"
            value={String(status.quietSecs)}
            onchange={(e) => onhistory({ kind: 'options', quietSecs: Number(e.currentTarget.value), syncOnOpenClose: status.syncOnOpenClose })}
          >
            {#each QUIET_CHOICES as choice (choice.secs)}<option value={String(choice.secs)}>{choice.label}</option>{/each}
            {#if !quietKnown}<option value={String(status.quietSecs)}>after {status.quietSecs} seconds without changes</option>{/if}
          </select>
        </label>

        {#if status.tooLarge.length > 0}
          <p class="quiet">Not kept in versions — larger than 50 MB:</p>
          <ul class="lines" data-testid="history-too-large">
            {#each status.tooLarge as file (file.rel)}<li><code>{file.rel}</code> ({bytesText(file.bytes)})</li>{/each}
          </ul>
        {/if}

        <div class="buttons">
          <button type="button" data-testid="history-version-now" disabled={busy} onclick={() => onhistory({ kind: 'versionNow' })}>
            Save a version now
          </button>
          <button
            type="button"
            data-testid="history-disable"
            disabled={busy}
            title="Stops saving versions. The history already saved stays where it is."
            onclick={() => onhistory({ kind: 'disable' })}
          >
            Turn off
          </button>
        </div>

        <h3>Sync</h3>
        {#if !status.syncOffered}
          <p class="quiet" data-testid="history-cloud">
            {status.cloud} already syncs this folder, so Jott does not offer a second sync — two syncers on the same files
            fight. Versions stay on this computer.
          </p>
        {:else if status.remote}
          <p class="quiet">Connected to <code data-testid="history-remote">{status.remote}</code>.</p>
          {#if status.lastSync}
            <p data-testid="history-sync-line" class:warn={status.lastSync.error !== null}>{syncSentence(status.lastSync, now)}</p>
            {#if status.lastSync.outcome?.kind === 'merged' && status.lastSync.outcome.keptBoth.length > 0}
              <ul class="lines" data-testid="history-kept-both">
                {#each status.lastSync.outcome.keptBoth as pair (pair.remoteCopy)}
                  <li><code>{pair.rel}</code> kept yours; the other computer’s text is in <code>{pair.remoteCopy}</code></li>
                {/each}
              </ul>
            {/if}
          {/if}
          <label class="row">
            <input
              type="checkbox"
              checked={status.syncOnOpenClose}
              data-testid="history-sync-open-close"
              onchange={() => onhistory({ kind: 'options', quietSecs: status.quietSecs, syncOnOpenClose: !status.syncOnOpenClose })}
            />
            <span>Also sync when Jott opens and closes this folder</span>
          </label>
          <div class="buttons">
            <button type="button" data-testid="history-sync" disabled={busy || status.identity === null} onclick={() => onhistory({ kind: 'sync' })}>
              {busy ? 'Working…' : 'Sync now'}
            </button>
            <button type="button" data-testid="history-disconnect" disabled={busy} onclick={() => onhistory({ kind: 'disconnect' })}>
              Disconnect
            </button>
          </div>
        {:else}
          <p class="quiet">
            Paste the address of an empty repository on GitHub, GitLab, Gitea or any git server. git asks for the login
            itself; Jott never keeps it.
          </p>
          <form class="connect" onsubmit={(e) => { e.preventDefault(); if (!urlHasLogin) onhistory({ kind: 'connect', url: url.trim() }) }}>
            <input
              placeholder="https://github.com/you/notes.git"
              bind:value={url}
              aria-label="Repository address"
              aria-invalid={urlHasLogin}
              data-testid="history-url"
            />
            <button type="submit" disabled={busy || !url.trim() || urlHasLogin} data-testid="history-connect">Connect</button>
          </form>
          {#if urlHasLogin}
            <p class="warn">This address has a login written into it. Use it without the login — git will ask for it.</p>
          {/if}
        {/if}
      {/if}
    {/if}

    <details class="advanced">
      <summary>Which git</summary>
      <form class="connect" onsubmit={(e) => { e.preventDefault(); onhistory({ kind: 'setGitPath', path: gitPath.trim() || null }) }}>
        <input
          placeholder={status.git.configured ?? (status.git.path ?? 'path to git')}
          bind:value={gitPath}
          aria-label="Path to git"
          data-testid="git-path"
        />
        <button type="submit" disabled={busy || !gitPath.trim()}>Use this git</button>
      </form>
      <div class="buttons">
        {#if status.git.configured}
          <button type="button" disabled={busy} onclick={() => onhistory({ kind: 'setGitPath', path: null })}>Search for git again</button>
        {/if}
        {#if status.git.found}
          <button type="button" disabled={busy} onclick={() => onhistory({ kind: 'checkGit' })}>Check again</button>
        {/if}
      </div>
    </details>
  {/if}
</fieldset>

<style>
  fieldset {
    border: 1px solid var(--line);
    border-radius: 5px;
    margin: 0.8rem 0 0;
    padding: 0.6rem 0.7rem 0.8rem;
  }

  legend {
    font-size: 0.82rem;
    padding: 0 0.3rem;
  }

  h3 {
    font-size: 0.86rem;
    margin: 0.9rem 0 0.2rem;
  }

  p {
    font-size: 0.88rem;
    margin: 0.3rem 0;
  }

  .quiet {
    font-size: 0.84rem;
    opacity: 0.85;
  }

  .warn {
    color: var(--warn-ink, #b45309);
  }

  .lines {
    margin: 0.2rem 0 0.5rem;
    padding-left: 1.1rem;
    font-size: 0.84rem;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }

  .row {
    display: flex;
    gap: 0.45rem;
    align-items: center;
    margin: 0.4rem 0;
    font-size: 0.88rem;
  }

  .identity,
  .connect {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    align-items: center;
    margin: 0.4rem 0;
  }

  .identity input {
    flex: 1 1 8rem;
  }

  .connect input {
    flex: 1 1 14rem;
  }

  input[aria-invalid='true'] {
    outline: 2px solid #b91c1c;
  }

  .advanced {
    margin-top: 0.7rem;
    font-size: 0.86rem;
  }
</style>
