<script lang="ts">
  // The window's search choices — engine, word beginnings, reranking — and the one permission, in one panel.
  //
  // It exists because the tester could not find them: "a settings menu to show this, so it isn't hidden" — a toggle
  // beside the search box is discoverable until you want the *other* setting, and after that there was nothing at all.
  // The rules themselves live in `searchSettings` and `consentHelp`, both tested; this file only asks.

  import { modeSentence, type SearchMode } from './searchSettings'
  import { cardClipboard, panelLines, type ConsentFacts } from './consentHelp'
  import { meaningsNotice } from './searchState'
  import { manualSteps, rerankerSummary, topNAccepted, topNValue } from './rerankStatus'
  import type { RerankerStatus } from './ipc'
  import type { HistoryStatus } from './ipc'
  import type { HistoryAction } from './history'
  import HistorySection from './HistorySection.svelte'
  import { openUrl } from '@tauri-apps/plugin-opener'

  let {
    mode,
    prefix,
    rerank,
    reranker = null,
    rerankerInstalling = false,
    vectors,
    cardFacts,
    /** True while the read that fills `cardFacts` is in flight. Without it, "no lines yet" cannot be told apart from
     *  "np's settings are complete", and the panel said *Reading…* forever — including after a read that had nothing
     *  to report, which is the state where somebody most wants to know it finished. */
    embeddingsLoading = false,
    consentBusy = false,
    onmode,
    onprefix,
    onrerank,
    onrerankretry,
    onrerankinstall,
    ontopn,
    onconsent,
    history = null,
    vaultOpen = true,
    historyBusy = false,
    historyError = null,
    onhistory = () => {},
    onClose,
  }: {
    mode: SearchMode
    prefix: boolean
    /** Whether a second step reorders the top results with note-processor's ranking model. */
    rerank: boolean
    /** The ranking model's standing for this folder; `null` until asked. */
    reranker?: RerankerStatus | null
    rerankerInstalling?: boolean
    /** What np said about vectors in the last search: `null` when nobody has searched yet, which is not the same as
     *  "no vectors" and must not be shown as such. */
    vectors: boolean | null
    /** Embeddings as core reads them, plus whatever np answered to the button. `null` while that is still in flight. */
    cardFacts: ConsentFacts | null
    embeddingsLoading?: boolean
    consentBusy?: boolean
    onmode: (mode: SearchMode) => void
    onprefix: (on: boolean) => void
    onrerank: (on: boolean) => void
    onrerankretry: () => void
    onrerankinstall: () => void
    ontopn: (n: number) => void
    onconsent: (granted: boolean) => void
    /** Versions and sync for the open folder; `null` until core has answered. */
    history?: HistoryStatus | null
    vaultOpen?: boolean
    historyBusy?: boolean
    historyError?: string | null
    onhistory?: (action: HistoryAction) => void
    onClose: () => void
  } = $props()

  const MODES: SearchMode[] = ['keyword', 'hybrid', 'semantic']
  // The copy affordance moved here from the card. That card used to carry key names, a block of TOML to paste and a
  // link into note-processor's tracker for somebody whose only decision was whether meaning search should work; this
  // panel is the surface that *does* carry the detail, so it keeps the way to get the text off the screen.
  let copied = $state(false)
  let copyFailed = $state(false)

  async function copyEmbeddings() {
    if (cardFacts === null) return
    copied = false
    try {
      await navigator.clipboard.writeText(cardClipboard(cardFacts))
      copied = true
      copyFailed = false
    } catch {
      // The clipboard refused. The text still has to leave this window somehow, so it goes below the button.
      copyFailed = true
    }
  }
  const notice = $derived(meaningsNotice(vectors))
  const embeddingsLines = $derived(cardFacts ? panelLines(cardFacts) : [])
  const rerankSummary = $derived(rerankerSummary(reranker, rerank, rerankerInstalling))
  // The field holds what is typed; np's value is only written when it is one np accepts.
  let topNDraft = $state<number | null>(null)
  const topNShown = $derived(topNDraft ?? topNValue(reranker))
  const topNValid = $derived(topNAccepted(topNShown))

  function commitTopN() {
    if (topNDraft === null || !topNValid || topNDraft === topNValue(reranker)) return
    ontopn(topNDraft)
    topNDraft = null
  }

  let panel = $state<HTMLElement | undefined>()
  $effect(() => {
    // Focus the panel rather than leaving it on the body: this opens from a keystroke, and a person who presses
    // Escape next expects the key to land somewhere inside what just appeared.
    panel?.focus()
  })
</script>

<div class="settings-scrim" role="presentation" onclick={onClose}></div>

<!-- `div` rather than `section`: Svelte's a11y check rightly refuses an interactive role on a landmark element, and
     About uses the same shape. -->
<div
  class="settings"
  role="dialog"
  aria-modal="true"
  aria-label="Settings"
  data-testid="settings"
  tabindex="-1"
  bind:this={panel}
>
  <header>
    <h2>Settings</h2>
    <button type="button" data-testid="close-settings" onclick={onClose}>Close</button>
  </header>

  <fieldset>
    <legend>How search matches</legend>
    {#each MODES as choice (choice)}
      <label class="mode">
        <input
          type="radio"
          name="search-mode"
          checked={choice === mode}
          data-testid="mode-{choice}"
          onchange={() => onmode(choice)}
        />
        <span>{modeSentence(choice)}</span>
      </label>
    {/each}
    {#if notice !== null}
      <!-- Said once, under the choice it warns about, from np's own `vectors` fact. Without it a person picks
           "meanings only", gets nothing, and reasonably concludes their notes are wrong. -->
      <p class="warn" data-testid="settings-notice">{notice}</p>
    {/if}
    <label class="prefix">
      <input type="checkbox" checked={prefix} data-testid="settings-prefix" onchange={() => onprefix(!prefix)} />
      <span>Match word beginnings — “grand” finds grandmother</span>
    </label>
  </fieldset>

  <fieldset data-testid="settings-rerank">
    <legend>Reranking</legend>
    <label class="prefix">
      <input type="checkbox" checked={rerank} data-testid="settings-rerank-toggle" onchange={() => onrerank(!rerank)} />
      <span>
        After each search, reorder the top results with note-processor’s ranking model. Which notes match does not
        change — only their order. The list appears at once and is reordered a few seconds later.
      </span>
    </label>
    <p class="quiet" data-testid="rerank-status">{rerankSummary.line}</p>
    {#if rerankSummary.manual && reranker}
      <!-- The by-hand path, for when note-processor cannot fetch the file itself (a proxy that inspects TLS is the case
           measured so far). np checks the chosen file against its pinned digest before it accepts it. -->
      <ol class="lines" data-testid="rerank-manual">
        {#each manualSteps(reranker) as step}<li>{step}</li>{/each}
      </ol>
      <input class="url" readonly value={reranker.manual.url} aria-label="Download address" onfocus={(e) => e.currentTarget.select()} />
      <p class="quiet digest">SHA-256 {reranker.manual.sha256}</p>
    {/if}
    {#if rerankSummary.manual || rerankSummary.retry}
      <div class="buttons">
        {#if rerankSummary.retry}
          <button type="button" data-testid="rerank-retry" onclick={onrerankretry}>Try the download again</button>
        {/if}
        {#if rerankSummary.manual && reranker}
          <button type="button" data-testid="rerank-open-url" onclick={() => void openUrl(reranker.manual.url).catch(() => {})}>
            Open the download in the browser
          </button>
          <button type="button" data-testid="rerank-install" disabled={rerankerInstalling} onclick={onrerankinstall}>
            Choose the downloaded file…
          </button>
        {/if}
      </div>
    {/if}
    <details class="advanced">
      <summary>Advanced</summary>
      <label class="topn">
        <span>Results the ranking model reorders</span>
        <input
          type="number"
          min="1"
          max="64"
          step="1"
          data-testid="rerank-topn"
          value={topNShown}
          aria-invalid={!topNValid}
          oninput={(e) => (topNDraft = e.currentTarget.valueAsNumber)}
          onchange={commitTopN}
        />
      </label>
      <p class="quiet">
        {#if topNValid}
          Default {reranker?.topNDefault ?? 5}. More results cost more time per search — about a quarter of a second
          each. Saved as <code>[search] rerank_top_n</code> in note-processor’s settings{reranker?.configPath ? ` (${reranker.configPath})` : ''}.
        {:else}
          note-processor accepts a whole number from 1 to 64.
        {/if}
      </p>
    </details>
  </fieldset>

  <fieldset>
    <legend>Embeddings</legend>
    {#if embeddingsLines.length > 0}
      <ul class="lines" data-testid="settings-embeddings">
        {#each embeddingsLines as line}<li>{line}</li>{/each}
      </ul>
    {:else if embeddingsLoading}
      <p class="quiet">Reading what note-processor has recorded…</p>
    {:else}
      <!-- Nothing to report is its own answer. It used to be drawn as a read still in flight, which stated a state
           no fact supported — and was what the panel said whenever the read had failed too. -->
      <p class="quiet" data-testid="settings-embeddings">
        Jott has not been able to read note-processor’s settings this session; nothing here has changed them.
      </p>
    {/if}
    {#if copyFailed}
      <textarea
        readonly
        rows="6"
        aria-label="Embeddings settings to copy"
        data-testid="copy-fallback"
        onfocus={(e) => e.currentTarget.select()}>{cardFacts ? cardClipboard(cardFacts) : ''}</textarea>
    {/if}
    <div class="buttons">
      <button type="button" data-testid="copy-embeddings" onclick={() => void copyEmbeddings()}>
        {copied ? 'Copied' : 'Copy these settings'}
      </button>
      <button
        type="button"
        data-testid="embeddings-allow"
        disabled={consentBusy}
        onclick={() => onconsent(true)}
        title="Asks note-processor to record its engine and model, then starts fetching straight away. The size is note-processor’s to quote; Jott does not guess it."
      >
        Allow the download
      </button>
      <button
        type="button"
        data-testid="embeddings-revoke"
        disabled={consentBusy}
        onclick={() => onconsent(false)}
        title="Asks note-processor to switch embeddings off and keeps Jott's download guardrail down."
      >
        Keep it keyword-only
      </button>
    </div>
  </fieldset>

  <HistorySection status={history} {vaultOpen} busy={historyBusy} error={historyError} {onhistory} />
</div>

<style>
  .settings-scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--ink) 28%, transparent);
    z-index: 40;
  }

  .settings {
    position: fixed;
    inset: 8vh auto auto 50%;
    transform: translateX(-50%);
    width: min(34rem, 92vw);
    max-height: 80vh;
    overflow: auto;
    background: var(--paper);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 8px 28px rgb(0 0 0 / 28%);
    padding: 0.9rem 1rem 1.1rem;
    z-index: 41;
  }

  .settings header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
  }

  .settings h2 {
    margin: 0;
    font-size: 1rem;
  }

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

  .mode,
  .prefix {
    display: flex;
    gap: 0.45rem;
    align-items: baseline;
    margin: 0.3rem 0;
  }

  .warn {
    color: var(--warn-ink, inherit);
    font-size: 0.84rem;
    margin: 0.5rem 0 0;
  }

  .lines {
    margin: 0.2rem 0 0.6rem;
    padding-left: 1.1rem;
    font-size: 0.86rem;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }

  .quiet {
    font-size: 0.86rem;
    margin: 0.2rem 0 0.4rem;
  }

  .url {
    width: 100%;
    font-size: 0.78rem;
    margin: 0.2rem 0;
  }

  .digest {
    font-size: 0.72rem;
    overflow-wrap: anywhere;
  }

  .advanced {
    margin-top: 0.6rem;
    font-size: 0.86rem;
  }

  .topn {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin: 0.4rem 0 0.2rem;
  }

  .topn input {
    width: 4.5rem;
  }

  .topn input[aria-invalid='true'] {
    outline: 2px solid #b91c1c;
  }
</style>
