<script lang="ts">
  // One line about the index, plus the two things a person can do about it (§9, §11). The wording comes
  // from `statusLine.ts`, which is tested; this file only puts it on screen.

  import type { Status } from './statusLine'
  import type { IngestEvent } from './ipc'

  let {
    status,
    consent = null,
    onembed,
    onconsent,
  }: {
    status: Status | null
    /** The `needsConsent` event, kept until it is answered. One prompt, not one per save (§8). */
    consent?: Extract<IngestEvent, { needsConsent: unknown }>['needsConsent'] | null
    onembed: () => void
    onconsent: (granted: boolean) => void
  } = $props()
</script>

<footer class="status" data-tone={status?.tone ?? 'neutral'}>
  <span class="text">
    {#if status}{status.text}{:else}idle{/if}
  </span>

  {#if status?.progress}
    <!-- A bar only ever drawn from np's own counters. No total means np did not know the size, which is drawn as
         movement without a number — a `0%` bar reads as a stalled download and would be a lie either way (§18.12). -->
    {#if status.progress.total !== null}
      <progress
        class="bar"
        value={status.progress.done}
        max={status.progress.total}
        aria-label={status.text}></progress>
    {:else}
      <div class="bar indeterminate" role="progressbar" aria-label={status.text}></div>
    {/if}
  {/if}

  {#if status?.action === 'consent' && consent}
    <span class="buttons">
      <button type="button" onclick={() => onconsent(true)}>Allow the download</button>
      <button type="button" onclick={() => onconsent(false)}>Stay keyword-only</button>
    </span>
  {:else}
    <button type="button" class="embed" onclick={onembed} title="Index and embed whatever is waiting, now">
      [embed]
    </button>
  {/if}
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.3rem 0.6rem;
    border-top: 1px solid color-mix(in srgb, currentColor 18%, transparent);
    font-size: 0.8125rem;
    color: color-mix(in srgb, currentColor 78%, transparent);
  }
  /* `busy` is movement, `warn` is a state worth noticing — neither is an error (§11). */
  .status[data-tone='busy'] .text {
    color: color-mix(in srgb, currentColor 92%, transparent);
  }
  .status[data-tone='warn'] .text {
    color: #b8863b;
  }
  :global(.dark) .status[data-tone='warn'] .text {
    color: #d9a441;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Slim, in the current colour, and never wider than the sentence beside it. */
  .bar {
    width: 7rem;
    height: 3px;
    accent-color: currentColor;
    flex: 0 0 auto;
  }
  .indeterminate {
    /* No number to show, so show movement: a stripe that travels the track. */
    background: repeating-linear-gradient(90deg, currentColor 0 25%, transparent 25% 50%) 0 0 / 200% 100%;
    border: none;
    animation: slide 1.4s linear infinite;
  }
  @keyframes slide {
    from {
      background-position: 0 0;
    }
    to {
      background-position: -200% 0;
    }
  }
  /* Nobody should have a stripe running circles around them while they read. The bytes keep arriving either way. */
  @media (prefers-reduced-motion: reduce) {
    .indeterminate {
      animation: none;
    }
  }
  .buttons,
  button {
    display: flex;
    gap: 0.4rem;
  }
  button {
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
    font: inherit;
    padding: 0.05rem 0.45rem;
    cursor: default;
  }
  button:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
</style>
