<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { isConnected, type PurposeFilter, type StatusFilter, type TypeFilter } from '../lib/filters';

  const base = $derived((app.view?.rows ?? []).filter((r) => app.showIgnored || !r.ignored));
  const counts = $derived({
    all: base.length,
    connected: base.filter((r) => isConnected(r.status)).length,
    disconnected: base.filter((r) => !isConnected(r.status)).length,
    usb: base.filter((r) => r.transport === 'usb').length,
    bluetooth: base.filter((r) => r.transport === 'bluetooth').length,
    virtual: base.filter((r) => r.transport === 'virtual').length,
    cat: base.filter((r) => r.purpose === 'cat' || r.purpose === 'secondary_cat').length,
    ptt: base.filter((r) => r.purpose === 'ptt').length,
    kiss: base.filter((r) => r.purpose === 'kiss').length,
    unknown: base.filter((r) => !r.purpose || r.purpose === 'unknown').length,
  });

  const statuses: [StatusFilter, string][] = [
    ['all', 'All'],
    ['connected', 'Connected'],
    ['disconnected', 'Disconnected'],
  ];
  const types: [TypeFilter, string][] = [
    ['usb', 'USB'],
    ['bluetooth', 'Bluetooth'],
    ['virtual', 'Virtual'],
  ];
  const purposes: [PurposeFilter, string][] = [
    ['cat', 'CAT'],
    ['ptt', 'PTT'],
    ['kiss', 'KISS'],
    ['unknown', 'Unknown'],
  ];
</script>

<div class="filters" role="toolbar" aria-label="Filters">
  <div class="group" role="radiogroup" aria-label="Connection state">
    {#each statuses as [value, label] (value)}
      <button
        class="chip"
        class:active={app.status === value}
        role="radio"
        aria-checked={app.status === value}
        onclick={() => (app.status = value)}
      >
        {label}<span class="n">{counts[value]}</span>
      </button>
    {/each}
  </div>
  <span class="sep" aria-hidden="true"></span>
  <div class="group" role="group" aria-label="Connection type">
    {#each types as [value, label] (value)}
      <button class="chip" class:active={app.types.includes(value)} aria-pressed={app.types.includes(value)} onclick={() => app.toggleType(value)}>
        {label}<span class="n">{counts[value]}</span>
      </button>
    {/each}
  </div>
  <span class="sep" aria-hidden="true"></span>
  <div class="group" role="group" aria-label="Purpose">
    {#each purposes as [value, label] (value)}
      <button
        class="chip"
        class:active={app.purposes.includes(value)}
        aria-pressed={app.purposes.includes(value)}
        onclick={() => app.togglePurpose(value)}
      >
        {label}<span class="n">{counts[value]}</span>
      </button>
    {/each}
  </div>
  {#if app.filtersActive}
    <button class="btn ghost small" onclick={() => app.clearFilters()}>Clear filters</button>
  {/if}
  <span class="shown">{app.rows.length} of {counts.all} shown</span>
</div>

<style>
  .filters {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 7px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .group {
    display: flex;
    gap: 4px;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--border);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
  }
  .chip:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }
  .chip.active {
    background: var(--accent-soft);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
    font-weight: 600;
  }
  .n {
    font-size: 11px;
    color: var(--muted);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .chip.active .n {
    color: inherit;
    opacity: 0.8;
  }
  .shown {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
</style>
