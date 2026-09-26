<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { isConnected } from '../lib/filters';
  import { timeOfDay } from '../lib/format';
  import Icon from './Icon.svelte';

  let popover = $state<'notices' | 'com' | null>(null);
  let root: HTMLElement | undefined = $state();

  const summary = $derived(app.view?.summary);
  const scan = $derived(app.view?.scan);
  const notices = $derived((app.view?.findings ?? []).filter((f) => f.deviceIds.length === 0));
  const reserved = $derived(scan?.reserved ?? null);

  type Cell = { n: number; state: 'connected' | 'hidden' | 'stale' | 'history' | 'free'; label: string; deviceId: number | null };

  const cells = $derived.by((): Cell[] => {
    if (!reserved || !app.view) return [];
    const byNumber = new Map<number, { state: Cell['state']; label: string; deviceId: number }>();
    for (const row of app.view.rows) {
      const m = /^COM(\d+)$/i.exec(row.port ?? '');
      if (!m) continue;
      const n = Number(m[1]);
      const state = isConnected(row.status) ? 'connected' : row.status === 'absent_os' ? 'hidden' : 'history';
      const rank = { connected: 3, hidden: 2, history: 1 } as const;
      const existing = byNumber.get(n);
      if (!existing || rank[state] > rank[existing.state as keyof typeof rank]) {
        byNumber.set(n, { state, label: row.nickname ?? row.deviceLabel, deviceId: row.deviceId });
      }
    }
    const reservedSet = new Set(reserved.numbers);
    const max = Math.max(16, ...reserved.numbers, ...byNumber.keys());
    const limit = Math.min(256, Math.ceil(max / 8) * 8);
    const out: Cell[] = [];
    for (let n = 1; n <= limit; n++) {
      const d = byNumber.get(n);
      if (d && d.state !== 'history') out.push({ n, state: d.state, label: d.label, deviceId: d.deviceId });
      else if (reservedSet.has(n))
        out.push({ n, state: 'stale', label: d ? `Reserved — last used by ${d.label}` : 'Reserved, no device', deviceId: d?.deviceId ?? null });
      else out.push({ n, state: d ? 'history' : 'free', label: d ? `Free — previously ${d.label}` : 'Free', deviceId: d?.deviceId ?? null });
    }
    return out;
  });

  function onWindowClick(e: MouseEvent) {
    if (popover && root && !root.contains(e.target as Node)) popover = null;
  }

  function toggle(which: 'notices' | 'com') {
    popover = popover === which ? null : which;
  }
</script>

<svelte:window onclick={onWindowClick} />

<footer class="statusbar" bind:this={root}>
  {#if summary}
    <span>
      <strong>{summary.total}</strong> ports · <span class="ok">{summary.connected} connected</span> · {summary.disconnected} disconnected{#if summary.ignored} · {summary.ignored} ignored{/if}
    </span>
    {#if summary.problems}
      <span class="item danger"><Icon name="alert" size={12} /> {summary.problems} with driver problems</span>
    {/if}
    {#if summary.warnings}
      <span class="item warn"><Icon name="alert" size={12} /> {summary.warnings} warnings</span>
    {/if}
    {#if notices.length}
      <button class="item" onclick={() => toggle('notices')} aria-expanded={popover === 'notices'}>
        <Icon name="info" size={12} /> {notices.length} notice{notices.length > 1 ? 's' : ''}
      </button>
    {/if}
    {#if reserved}
      <button class="item" onclick={() => toggle('com')} aria-expanded={popover === 'com'} title="Show which COM numbers are in use or reserved">
        COM numbers: {reserved.numbers.length} reserved
      </button>
    {/if}
  {/if}
  <span class="spacer"></span>
  {#if app.view?.storage.temporary}
    <span class="item warn" title={app.view.storage.reason ?? ''}><Icon name="alert" size={12} /> Changes are not being saved</span>
  {/if}
  {#if app.updateBadge}
    <button class="item accent" onclick={() => (app.dialog = 'about')}>
      <Icon name="download" size={12} />
      {app.update?.phase === 'ready_to_restart' ? 'Restart to finish updating' : `Update ${app.update?.available?.version ?? ''} available`}
    </button>
  {/if}
  {#if scan}
    <span class="live" title={`Live monitoring: ${scan.capabilities.monitoring}`}><span class="pulse"></span>Live</span>
    <span class="muted" title={`Last scan took ${scan.durationMs} ms`}>Updated {timeOfDay(scan.scannedAt)}</span>
  {/if}

  {#if popover === 'notices'}
    <div class="popover">
      {#each notices as f (f.code)}
        <div class="notice">
          <strong>{f.title}</strong>
          <p>{f.detail}</p>
        </div>
      {/each}
    </div>
  {:else if popover === 'com' && reserved}
    <div class="popover com">
      <div class="com-head">
        <strong>COM number map</strong>
        <span class="muted">{reserved.source}</span>
      </div>
      <div class="grid">
        {#each cells as c (c.n)}
          <button
            class="cell {c.state}"
            title={`COM${c.n}: ${c.label}`}
            disabled={c.deviceId == null}
            onclick={() => {
              if (c.deviceId != null) {
                app.reveal(c.deviceId);
                popover = null;
              }
            }}>{c.n}</button
          >
        {/each}
      </div>
      <div class="legend">
        <span><i class="cell connected"></i>Connected</span>
        <span><i class="cell hidden"></i>Hidden device</span>
        <span><i class="cell stale"></i>Reserved, unused</span>
        <span><i class="cell free"></i>Free</span>
      </div>
      <p class="muted small">
        Windows reserves a COM number for every serial device it has seen. Numbers held by hidden devices and stale
        reservations push new devices to higher numbers.
      </p>
    </div>
  {/if}
</footer>

<style>
  .statusbar {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 28px;
    padding: 0 12px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .ok {
    color: var(--ok);
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 6px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    font-size: 12px;
  }
  button.item:hover {
    background: var(--surface-3);
  }
  .item.danger {
    color: var(--danger);
  }
  .item.warn {
    color: var(--warn);
  }
  .item.accent {
    color: var(--accent);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    animation: pulse 2.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  .popover {
    position: absolute;
    left: 12px;
    bottom: 32px;
    z-index: 40;
    width: 460px;
    max-height: 60vh;
    overflow: auto;
    padding: 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    white-space: normal;
    color: var(--text);
  }
  .notice + .notice {
    margin-top: 10px;
  }
  .notice p {
    margin: 3px 0 0;
    color: var(--text-2);
  }
  .com-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
  }
  .cell {
    height: 24px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font: 11px var(--mono);
    padding: 0;
  }
  .cell.connected {
    background: var(--ok-soft);
    border-color: color-mix(in srgb, var(--ok) 40%, transparent);
    color: var(--ok);
    font-weight: 600;
  }
  .cell.hidden {
    background: var(--surface-3);
    border-style: dashed;
    border-color: var(--border-strong);
    color: var(--text-2);
  }
  .cell.stale {
    background: var(--warn-soft);
    border-color: color-mix(in srgb, var(--warn) 45%, transparent);
    color: var(--warn);
  }
  .cell.history {
    color: var(--text-2);
  }
  .cell:not(:disabled) {
    cursor: pointer;
  }
  .legend {
    display: flex;
    gap: 12px;
    margin: 10px 0 6px;
    font-size: 11.5px;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .legend i {
    display: inline-block;
    width: 12px;
    height: 12px;
  }
  .small {
    font-size: 11.5px;
    margin: 0;
  }
</style>
