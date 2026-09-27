<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { timeOfDay } from '../lib/format';
  import type { PortRow } from '../lib/types';
  import { holderName, holderNames, programName, span } from '../lib/usage';
  import Icon from './Icon.svelte';

  let { row }: { row: PortRow } = $props();

  const usage = $derived(app.usageFor(row));
  const holders = $derived(usage?.holders ?? []);
  const exiting = $derived(holders.length > 0 && holders.every((h) => h.exiting));
  const portName = $derived(row.portShort ?? row.port ?? 'The port');
  const waitText = $derived.by(() => {
    const then = usage?.watch?.thenOpen;
    const start = then ? ` and ComInspect will start ${programName(then)}` : '';
    return `You will get a notification${start} as soon as ${holderNames(holders) || 'the program'} lets go of it. Keep ComInspect running; minimized is fine.`;
  });

  /** Program to start once the port is free (remembered per device). */
  let program = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    const id = row.deviceId;
    let current = true;
    program = null;
    void app.watchProgram(id).then((p) => {
      if (current) program = p;
    });
    return () => {
      current = false;
    };
  });

  function since(at: number, exact: boolean): string {
    return exact ? `since ${timeOfDay(at)} (${span(app.now - at)})` : `since ${timeOfDay(at)} or earlier`;
  }

  async function choose() {
    const picked = await app.pickProgram();
    if (picked) program = picked;
  }

  async function watch() {
    busy = true;
    try {
      await app.watchPort(row, program);
    } finally {
      busy = false;
    }
  }
</script>

{#if !usage}
  <p class="line muted">Checking which programs have this port open…</p>
{:else if usage.state === 'in_use'}
  <ul class="holders">
    {#each holders as h (h.pid)}
      <li title={h.executable ?? h.processName}>
        <div class="name">
          {holderName(h)}
          {#if h.exiting}<span class="pill warn">shutting down</span>{/if}
        </div>
        <div class="meta mono">{h.processName} · PID {h.pid}</div>
      </li>
    {/each}
  </ul>
  <p class="line muted small">Open {since(usage.since, usage.sinceExact)}.</p>
  {#if exiting}
    <p class="line small">
      {holderNames(holders)} is closing. Windows frees the port once it has finished; this can take a minute.
    </p>
  {/if}

  {#if usage.watch}
    <div class="callout waiting">
      <Icon name="bell" size={15} />
      <div>
        <strong>Waiting for {portName} to be free.</strong>
        {waitText}
      </div>
    </div>
    <div class="actions">
      <button class="btn small" onclick={() => row.port && app.unwatchPort(row.port)}>Stop waiting</button>
      <span class="muted small">Waiting since {timeOfDay(usage.watch.armedAt)}</span>
    </div>
  {:else}
    <div class="then">
      {#if program}
        <Icon name="play" size={12} />
        <span class="then-text">Then start <strong title={program}>{programName(program)}</strong></span>
        <button class="btn ghost small" onclick={choose}>Change…</button>
        <button class="btn ghost small" onclick={() => (program = null)}>Don't start</button>
      {:else}
        <button class="btn ghost small" onclick={choose}>
          <Icon name="play" size={12} /> Also start a program when it's free…
        </button>
      {/if}
    </div>
    <div class="actions">
      <button class="btn primary small" disabled={busy} onclick={watch}>
        <Icon name="bell" size={13} /> Notify me when it's free
      </button>
    </div>
  {/if}
{:else if usage.state === 'free'}
  <p class="line">
    <span class="ok"><Icon name="check" size={13} /></span>
    Not in use by any program <span class="muted small">{since(usage.since, usage.sinceExact)}</span>
  </p>
  {#if app.usage?.limitation}
    <p class="line muted small">{app.usage.limitation}</p>
  {/if}
{:else}
  <p class="line muted">Could not check which programs use this port{usage.reason ? `: ${usage.reason}` : ''}.</p>
{/if}

<style>
  .line {
    margin: 0 0 6px;
  }
  .small {
    font-size: 11.5px;
  }
  .ok {
    display: inline-flex;
    vertical-align: -2px;
    color: var(--ok);
  }
  .holders {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .holders li {
    padding: 6px 9px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .meta {
    margin-top: 1px;
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .callout.waiting {
    margin: 8px 0 6px;
  }
  .callout.waiting :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--accent);
  }
  .then {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px 4px;
    margin: 8px 0 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .then > :global(svg) {
    color: var(--ok);
  }
  .then-text {
    margin-right: 2px;
  }
  .then .btn.ghost:only-child {
    margin-left: -8px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
