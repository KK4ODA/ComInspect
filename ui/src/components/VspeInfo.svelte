<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { isConnected } from '../lib/filters';
  import type { PortRow } from '../lib/types';
  import { holderNames } from '../lib/usage';
  import { dateTime } from '../lib/format';
  import { portList, sourceLabel, splitterOf } from '../lib/vspe';

  let { row }: { row: PortRow } = $props();

  const key = $derived(row.port?.toUpperCase() ?? '');
  const split = $derived(splitterOf(row, app.links));
  const siblings = $derived(split ? split.ports.filter((p) => p.toUpperCase() !== key) : []);
  const shared = $derived(key ? (app.links.sharedBy.get(key) ?? []) : []);
  const roles = $derived(key ? (app.links.roles.get(key) ?? []) : []);
  const file = $derived(app.vspe?.file ?? null);
  const label = $derived(app.vspe ? sourceLabel(app.vspe) : "VSPE's startup configuration");
  const saved = $derived(app.vspe?.modifiedAt ? dateTime(app.vspe.modifiedAt) : null);
  const linked = $derived(!!split || shared.length > 0 || roles.length > 0);

  /** The row for a port, preferring a connected device. */
  function rowFor(port: string): PortRow | null {
    const matches = (app.view?.rows ?? []).filter((r) => r.port?.toUpperCase() === port.toUpperCase());
    return matches.find((r) => isConnected(r.status)) ?? matches[0] ?? null;
  }

  function nameOf(port: string): string | null {
    const r = rowFor(port);
    return r ? (r.nickname ?? r.deviceLabel) : null;
  }

  /** "in use by WSJT-X", from the port usage check. */
  function useOf(port: string): string | null {
    const usage = app.usage?.ports[port];
    return usage?.state === 'in_use' && usage.holders?.length ? `in use by ${holderNames(usage.holders)}` : null;
  }

  function select(port: string) {
    const r = rowFor(port);
    if (r) app.select(r.deviceId);
  }
</script>

{#snippet portButton(port: string)}
  {@const target = rowFor(port)}
  {@const name = nameOf(port)}
  {@const use = useOf(port)}
  <li>
    <button class="port-link" disabled={!target} onclick={() => select(port)} title={target ? `Show ${port}` : `${port} is not on this computer`}>
      <span class="dot {target?.status ?? 'absent'}"></span>
      <span class="mono">{port}</span>
      {#if name}<span class="name">{name}</span>{/if}
    </button>
    {#if use}<span class="use">{use}</span>{:else if !target}<span class="use">not found</span>{/if}
  </li>
{/snippet}

{#if split}
  <p class="line">Virtual port of a VSPE <strong>splitter</strong> that shares:</p>
  <ul class="ports">{@render portButton(split.source)}</ul>
  {#if siblings.length}
    <p class="line muted small">The splitter also shares it as {portList(siblings)}.</p>
  {/if}
{/if}

{#each shared as s, i (i)}
  <p class="line">Shared by a VSPE <strong>splitter</strong> as:</p>
  <ul class="ports">
    {#each s.ports as port (port)}{@render portButton(port)}{/each}
  </ul>
  {#if s.baud}
    <p class="line muted small">VSPE opens {row.portShort ?? row.port} at {s.baud} baud and passes the data to every program on these ports.</p>
  {/if}
{/each}

{#each roles as role, i (i)}
  {#if role.kind === 'pair'}
    <p class="line">Connected to <strong>{role.peer}</strong> by a VSPE <strong>pair</strong>: what a program writes here, the program on {role.peer} reads, and back.</p>
    <ul class="ports">{@render portButton(role.peer)}</ul>
  {:else if role.kind === 'redirector'}
    <p class="line">VSPE copies data between this port and <strong>{role.peer}</strong> (redirector).</p>
    <ul class="ports">{@render portButton(role.peer)}</ul>
  {:else if role.kind === 'connector'}
    <p class="line">A VSPE <strong>connector</strong>: a virtual port that two programs open to talk to each other.</p>
  {:else if role.kind === 'network'}
    <p class="line">Shared on the network by VSPE: <strong>{role.protocol}</strong>, {role.address}.</p>
  {/if}
{/each}

{#if !linked}
  {#if !file}
    <p class="line">ComInspect can show how this VSPE port is connected once it can read your VSPE configuration.</p>
  {:else if !app.vspe?.error}
    <p class="line warn">
      This port isn't in {label}{#if saved}, saved {saved}{/if}. If you set it up in VSPE since then, save the configuration
      again in VSPE, or read a newer file.
    </p>
  {/if}
{/if}

{#if app.vspe?.error}
  <p class="line warn small">{app.vspe.error}</p>
{/if}

<div class="source">
  <span class="muted small" title={file ?? undefined}>
    {#if !file}
      No VSPE configuration found.
    {:else}
      From {label}{#if saved}, saved {saved}{/if}
    {/if}
  </span>
  <button class="btn ghost small" onclick={() => (app.dialog = 'vspe')}>Change…</button>
</div>

<style>
  .line {
    margin: 0 0 6px;
  }
  .small {
    font-size: 11.5px;
  }
  .warn {
    color: var(--warn);
  }
  .ports {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ports li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .port-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 100%;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    text-align: left;
  }
  .port-link:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .port-link:disabled {
    opacity: 0.7;
  }
  .port-link .mono {
    font-weight: 600;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .use {
    flex: none;
    font-size: 11.5px;
    color: var(--muted);
  }
  .source {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px 6px;
    margin-top: 4px;
  }
</style>
