<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { isConnected, type SortColumn } from '../lib/filters';
  import { isRecent, relativeTime, dateTime, timeOfDay, vidPid } from '../lib/format';
  import { STATUS_TEXT, TRANSPORTS, purposeLabel } from '../lib/labels';
  import type { PortRow, PortUsageView } from '../lib/types';
  import { usageSummary } from '../lib/usage';
  import Icon from './Icon.svelte';

  let { editTick = 0 }: { editTick?: number } = $props();

  const columns: { key: SortColumn; label: string; cls: string; title?: string }[] = [
    { key: 'status', label: '', cls: 'c-status', title: 'Status' },
    { key: 'port', label: 'Port', cls: 'c-port' },
    { key: 'nickname', label: 'Nickname', cls: 'c-nick' },
    { key: 'device', label: 'Device', cls: 'c-device' },
    { key: 'type', label: 'Type', cls: 'c-type' },
    { key: 'vidpid', label: 'VID:PID', cls: 'c-vidpid' },
    { key: 'serial', label: 'Serial', cls: 'c-serial' },
    { key: 'purpose', label: 'Purpose', cls: 'c-purpose' },
    { key: 'lastSeen', label: 'Last Seen', cls: 'c-seen' },
  ];

  let width = $state(1200);
  let editing = $state<number | null>(null);
  let draft = $state('');

  const compact = $derived(width < 960 ? (width < 820 ? 2 : 1) : 0);

  $effect(() => {
    if (editTick > 0 && app.selectedRow) startEdit(app.selectedRow);
  });

  function startEdit(row: PortRow) {
    editing = row.deviceId;
    draft = row.nickname ?? '';
  }

  async function commit(row: PortRow) {
    if (editing !== row.deviceId) return;
    editing = null;
    const value = draft.trim();
    if (value !== (row.nickname ?? '')) {
      await app.updateIdentity(row.deviceId, { nickname: value || null });
    }
  }

  function focusInput(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function lastSeenText(row: PortRow): string {
    if (isConnected(row.status)) return 'Now';
    if (row.status === 'awaiting') return 'Not yet';
    return relativeTime(row.lastSeen, app.now);
  }

  function lastSeenTitle(row: PortRow): string {
    if (isConnected(row.status)) return 'Connected now';
    if (row.lastSeen == null) return 'Never seen connected by ComInspect';
    const source = row.lastSeenSource === 'os' ? ' (reported by the operating system)' : '';
    return `${dateTime(row.lastSeen)}${source}`;
  }

  function typeTitle(row: PortRow): string {
    if (row.transport === 'virtual' && row.virtualProvider) return `Virtual port (${row.virtualProvider})`;
    if (row.transport === 'bluetooth' && row.btAddress) return `Bluetooth ${row.btAddress}`;
    return TRANSPORTS[row.transport];
  }

  function usageTitle(usage: PortUsageView): string {
    let text = `${usageSummary(usage)} since ${timeOfDay(usage.since)}${usage.sinceExact ? '' : ' or earlier'}.`;
    if (usage.watch) text += ' ComInspect will tell you when it is free.';
    return text;
  }

  function sortIndicator(key: SortColumn): string {
    if (app.sort.column !== key) return '';
    return app.sort.dir === 1 ? '▲' : '▼';
  }
</script>

<div class="wrap" bind:clientWidth={width} data-compact={compact}>
  <table>
    <thead>
      <tr>
        {#each columns as col (col.key)}
          <th
            class={col.cls}
            class:sorted={app.sort.column === col.key}
            aria-sort={app.sort.column === col.key ? (app.sort.dir === 1 ? 'ascending' : 'descending') : 'none'}
            title={col.title ?? `Sort by ${col.label}`}
          >
            <button onclick={() => app.setSort(col.key)}>
              {col.label}<span class="arrow">{sortIndicator(col.key)}</span>
            </button>
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each app.rows as row (row.deviceId)}
        {@const connected = isConnected(row.status)}
        {@const usage = app.usageFor(row)}
        <tr
          data-device={row.deviceId}
          class:selected={row.deviceId === app.selectedId}
          class:offline={!connected}
          class:ignored={row.ignored}
          onclick={() => app.select(row.deviceId)}
          ondblclick={() => startEdit(row)}
          aria-selected={row.deviceId === app.selectedId}
        >
          <td class="c-status">
            <span class="status-cell" title={STATUS_TEXT[row.status]}>
              <span class="dot {row.status}"></span>
              {#if row.severity === 'error'}
                <Icon name="alert" size={13} class="sev error" title="Problem" />
              {:else if row.severity === 'warning'}
                <Icon name="alert" size={13} class="sev warning" title="Warning" />
              {/if}
            </span>
          </td>
          <td class="c-port">
            <span class="port mono">{row.portShort ?? '—'}</span>
            {#if usage?.state === 'in_use'}
              {@const closing = !!usage.holders?.length && usage.holders.every((h) => h.exiting)}
              <span class="pill usage" class:watched={!!usage.watch || closing} title={usageTitle(usage)}
                >{#if usage.watch}<Icon name="bell" size={11} />{/if}{closing ? 'closing' : 'in use'}</span
              >
            {:else if row.previousPort && isRecent(row.portChangedAt, app.now)}
              <span class="was" title={`Changed from ${row.previousPort} ${relativeTime(row.portChangedAt, app.now).toLowerCase()}`}
                >was {row.previousPort}</span
              >
            {/if}
          </td>
          <td class="c-nick">
            {#if editing === row.deviceId}
              <input
                class="edit"
                type="text"
                bind:value={draft}
                use:focusInput
                maxlength="120"
                placeholder="Nickname"
                onclick={(e) => e.stopPropagation()}
                onblur={() => commit(row)}
                onkeydown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter') commit(row);
                  if (e.key === 'Escape') editing = null;
                }}
              />
            {:else if row.nickname}
              <span class="nick" title={row.equipment ? `${row.nickname} — ${row.equipment}` : row.nickname}>{row.nickname}</span>
            {:else}
              <span class="placeholder" title="Double-click to name this port">Add nickname</span>
            {/if}
          </td>
          <td class="c-device" title={[row.deviceLabel, row.manufacturer].filter(Boolean).join(' — ')}>
            {#if row.hintLabel}<span class="pill hint">{row.hintLabel}</span>{/if}
            <span class="device">{row.deviceLabel}</span>
          </td>
          <td class="c-type" title={typeTitle(row)}>{TRANSPORTS[row.transport]}</td>
          <td class="c-vidpid mono">{vidPid(row.vid, row.pid)}</td>
          <td class="c-serial mono" title={row.serialNumber ?? ''}>{row.serialNumber ?? '—'}</td>
          <td class="c-purpose">
            {#if row.purpose && row.purpose !== 'unknown'}
              {purposeLabel(row.purpose)}
              {#if row.catStatus === 'verified'}
                <span class="verified" title="Verified CAT port"><Icon name="check" size={12} /></span>
              {/if}
            {:else if row.catStatus === 'verified'}
              CAT <span class="verified" title="Verified CAT port"><Icon name="check" size={12} /></span>
            {:else}
              <span class="muted">—</span>
            {/if}
          </td>
          <td class="c-seen" class:now={connected} title={lastSeenTitle(row)}>
            {lastSeenText(row)}{#if row.lastSeenSource === 'os' && !connected}<span class="os" title="Reported by the operating system">*</span>{/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  {#if app.view && app.rows.length === 0}
    <div class="empty">
      {#if app.view.rows.length === 0}
        <Icon name="plug" size={28} />
        <p><strong>No serial ports found.</strong></p>
        <p class="muted">
          Plug in a radio, interface or USB-serial cable — it will appear here automatically. Ports you have used before
          are remembered even when they are unplugged.
        </p>
      {:else}
        <p><strong>No ports match the current filters.</strong></p>
        <button class="btn" onclick={() => { app.clearFilters(); app.setShowIgnored(false); }}>Clear filters</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    overflow: auto;
    background: var(--surface);
  }
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    table-layout: fixed;
  }
  thead th {
    position: sticky;
    top: 0;
    z-index: 2;
    height: 30px;
    padding: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    text-align: left;
    font-weight: 600;
    font-size: 12px;
    color: var(--text-2);
  }
  th button {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  th button:hover {
    color: var(--text);
  }
  th.sorted {
    color: var(--text);
  }
  .arrow {
    font-size: 8px;
    color: var(--accent);
  }
  td {
    height: 30px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
  }
  tbody tr {
    cursor: default;
  }
  tbody tr:hover td {
    background: var(--row-hover);
  }
  tbody tr.selected td {
    background: var(--row-selected);
  }
  tr.offline td {
    color: var(--text-2);
  }
  tr.offline .nick {
    color: var(--text-2);
  }
  tr.ignored td {
    opacity: 0.55;
  }

  .c-status {
    width: 46px;
  }
  .c-port {
    width: 136px;
  }
  .c-nick {
    width: 19%;
  }
  .c-device {
    width: auto;
  }
  .c-type {
    width: 80px;
  }
  .c-vidpid {
    width: 86px;
  }
  .c-serial {
    width: 112px;
  }
  .c-purpose {
    width: 106px;
  }
  .c-seen {
    width: 110px;
  }
  [data-compact='1'] .c-serial,
  [data-compact='2'] .c-serial,
  [data-compact='2'] .c-vidpid {
    display: none;
  }

  .status-cell {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding-left: 4px;
  }
  .status-cell :global(.sev.error) {
    color: var(--danger);
  }
  .status-cell :global(.sev.warning) {
    color: var(--warn);
  }
  .port {
    font-weight: 600;
    font-size: 12.5px;
  }
  tr.offline .port {
    font-weight: 500;
  }
  .usage {
    margin-left: 6px;
    vertical-align: middle;
    background: var(--accent-soft);
    color: var(--accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, currentColor 25%, transparent);
  }
  .usage.watched {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .was {
    margin-left: 5px;
    font-size: 11px;
    color: var(--warn);
  }
  .nick {
    font-weight: 600;
  }
  .placeholder {
    color: var(--muted);
    font-style: italic;
    opacity: 0;
  }
  tr:hover .placeholder,
  tr.selected .placeholder {
    opacity: 0.8;
  }
  .edit {
    width: 100%;
    height: 24px;
  }
  .c-device .device {
    color: var(--text-2);
  }
  .pill.hint {
    margin-right: 6px;
  }
  .verified {
    display: inline-flex;
    vertical-align: -2px;
    color: var(--ok);
  }
  .c-seen {
    color: var(--text-2);
  }
  .c-seen.now {
    color: var(--ok);
    font-weight: 600;
  }
  .os {
    color: var(--muted);
    margin-left: 1px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 48px 24px;
    text-align: center;
    color: var(--text-2);
  }
  .empty p {
    margin: 0;
    max-width: 440px;
  }
</style>
