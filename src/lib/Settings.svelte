<script lang="ts">
  // The window's two search choices and the one permission, in one panel.
  //
  // It exists because the tester could not find them: "a settings menu to show this, so it isn't hidden" — a toggle
  // beside the search box is discoverable until you want the *other* setting, and after that there was nothing at all.
  // The rules themselves live in `searchSettings` and `consentHelp`, both tested; this file only asks.

  import { modeSentence, type SearchMode } from './searchSettings'
  import { panelLines, type ConsentFacts } from './consentHelp'
  import { meaningsNotice } from './searchState'

  let {
    mode,
    prefix,
    vectors,
    cardFacts,
    /** True while the read that fills `cardFacts` is in flight. Without it, "no lines yet" cannot be told apart from
     *  "np's settings are complete", and the panel said *Reading…* forever — including after a read that had nothing
     *  to report, which is the state where somebody most wants to know it finished. */
    embeddingsLoading = false,
    consentBusy = false,
    onmode,
    onprefix,
    onconsent,
    onClose,
  }: {
    mode: SearchMode
    prefix: boolean
    /** What np said about vectors in the last search: `null` when nobody has searched yet, which is not the same as
     *  "no vectors" and must not be shown as such. */
    vectors: boolean | null
    /** Embeddings as core reads them, plus whatever np answered to the button. `null` while that is still in flight. */
    cardFacts: ConsentFacts | null
    embeddingsLoading?: boolean
    consentBusy?: boolean
    onmode: (mode: SearchMode) => void
    onprefix: (on: boolean) => void
    onconsent: (granted: boolean) => void
    onClose: () => void
  } = $props()

  const MODES: SearchMode[] = ['keyword', 'hybrid', 'semantic']
  const notice = $derived(meaningsNotice(vectors))
  const embeddingsLines = $derived(cardFacts ? panelLines(cardFacts) : [])

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
    <div class="buttons">
      <button
        type="button"
        data-testid="embeddings-allow"
        disabled={consentBusy}
        onclick={() => onconsent(true)}
        title="Asks note-processor to record the model and switch fetching on. It downloads about 296 MiB on a first run."
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
</style>
