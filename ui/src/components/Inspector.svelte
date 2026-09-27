<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { isConnected } from '../lib/filters';
  import { dateTime, hex4, relativeTime } from '../lib/format';
  import { EVENT_TEXT, KEY_KIND_LABEL, STATUS_TEXT, TRANSPORTS, categoryLabel, purposeLabel } from '../lib/labels';
  import type { Category, Hint, Purpose } from '../lib/types';
  import Diagnostics from './Diagnostics.svelte';
  import Icon from './Icon.svelte';
  import IdentityEditor from './IdentityEditor.svelte';
  import KV from './KV.svelte';
  import PortUsage from './PortUsage.svelte';
  import Section from './Section.svelte';

  const row = $derived(app.selectedRow);
  const detail = $derived(app.detail && app.detail.row.deviceId === app.selectedId ? app.detail : null);
  const snap = $derived(detail?.snapshot ?? null);
  const usb = $derived(snap?.usb ?? null);
  const sys = $derived(snap?.system ?? null);
  const problems = $derived((detail?.findings ?? []).filter((f) => f.severity !== 'info'));
  const notes = $derived((detail?.findings ?? []).filter((f) => f.severity === 'info'));
  const isWindows = $derived(app.view?.scan?.platform === 'windows');
  const usage = $derived(app.usageFor(row));
  const usageBadge = $derived(
    usage?.watch ? 'waiting' : usage?.state === 'in_use' ? 'in use' : usage?.state === 'free' ? 'free' : null,
  );

  function applyHint(h: Hint) {
    if (!row) return;
    const patch: { purpose?: Purpose; category?: Category; equipment?: string } = {};
    if (h.suggestedPurpose && !row.purpose) patch.purpose = h.suggestedPurpose;
    if (h.suggestedCategory && !row.category) patch.category = h.suggestedCategory;
    if (h.suggestedEquipment && !row.equipment) patch.equipment = h.suggestedEquipment;
    void app.updateIdentity(row.deviceId, patch);
  }

  function applicable(h: Hint): string[] {
    if (!row) return [];
    const out: string[] = [];
    if (h.suggestedPurpose && !row.purpose) out.push(`purpose ${purposeLabel(h.suggestedPurpose)}`);
    if (h.suggestedCategory && !row.category) out.push(`category ${categoryLabel(h.suggestedCategory)}`);
    if (h.suggestedEquipment && !row.equipment) out.push(`equipment ${h.suggestedEquipment}`);
    return out;
  }

  function forget() {
    if (!row) return;
    const name = row.nickname ?? row.deviceLabel;
    app.confirm({
      title: 'Forget this device?',
      message: `"${name}" and its history will be removed from ComInspect. ${
        isConnected(row.status) ? 'Because it is connected it will reappear immediately as a new, unnamed entry.' : ''
      } This does not change anything in the operating system.`,
      confirmLabel: 'Forget device',
      danger: true,
      onConfirm: () => app.forget(row.deviceId),
    });
  }

  const statusClass = $derived(row ? row.status : 'absent');
</script>

{#if row}
  <aside class="inspector" aria-label="Port details">
    <header>
      <div class="titles">
        <h2 title={row.nickname ?? row.deviceLabel}>{row.nickname ?? row.deviceLabel}</h2>
        <div class="sub">
          <span class="dot {statusClass}"></span>
          <span class="mono port">{row.portShort ?? '—'}</span>
          <span>·</span>
          <span>{STATUS_TEXT[row.status]}</span>
        </div>
        {#if row.nickname}<div class="device-line">{row.deviceLabel}</div>{/if}
      </div>
      <button class="icon-btn" aria-label="Close details" onclick={() => app.select(null)}><Icon name="x" /></button>
    </header>

    <div class="body">
      {#each problems as f (f.code + f.title)}
        <div class="callout {f.severity === 'error' ? 'danger' : 'warn'} finding">
          <Icon name="alert" size={15} />
          <div><strong>{f.title}</strong><br />{f.detail}</div>
        </div>
      {/each}

      {#if app.usage && isConnected(row.status)}
        <Section key="usage" title="Programs using this port" badge={usageBadge}>
          <PortUsage {row} />
        </Section>
      {/if}

      <Section key="identity" title="User identity">
        <IdentityEditor {row} />
      </Section>

      {#if detail && detail.hints.length}
        <Section key="hints" title="Suggestions" badge={detail.hints.length}>
          <div class="hints">
            {#each detail.hints as h (h.id)}
              {@const apply = applicable(h)}
              <div class="hint" class:caution={h.caution}>
                <div class="hint-title">{#if h.caution}<Icon name="alert" size={13} />{/if}{h.title}</div>
                {#if h.detail}<div class="hint-detail">{h.detail}</div>{/if}
                {#if apply.length}
                  <button class="btn small" onclick={() => applyHint(h)}>Apply: {apply.join(', ')}</button>
                {/if}
              </div>
            {/each}
            <div class="muted small">Suggestions come from ComInspect's device database and are never applied automatically.</div>
          </div>
        </Section>
      {/if}

      <Section key="connection" title="Current connection">
        <KV
          items={[
            { label: 'Port', value: row.port, mono: true, copy: true },
            { label: 'Status', value: STATUS_TEXT[row.status] },
            { label: 'Aliases', value: snap?.aliases ?? [], mono: true },
            { label: 'Connected at', value: isConnected(row.status) && snap?.osTimes.lastArrival ? dateTime(snap.osTimes.lastArrival) : null },
            { label: 'Kernel device', value: sys?.kernelName, mono: true },
            { label: 'Previous port', value: row.previousPort ? `${row.previousPort}${row.portChangedAt ? ` (until ${relativeTime(row.portChangedAt, app.now).toLowerCase()})` : ''}` : null },
            { label: 'Access', value: sys?.access ? `${sys.access.readable && sys.access.writable ? 'read/write' : sys.access.readable ? 'read only' : 'no access'} · ${sys.access.mode ?? ''} ${sys.access.ownerGroup ? `(group ${sys.access.ownerGroup})` : ''}` : null },
          ]}
        />
        {#each notes as f (f.code + f.title)}
          <div class="note"><Icon name="info" size={13} /><div><strong>{f.title}.</strong> {f.detail}</div></div>
        {/each}
      </Section>

      <Section key="hardware" title="Hardware">
        <KV
          items={[
            { label: 'Type', value: TRANSPORTS[row.transport] },
            { label: 'Manufacturer', value: row.manufacturer ?? usb?.manufacturer },
            { label: 'Device', value: snap?.description ?? row.deviceLabel },
            { label: 'Product', value: usb?.product ?? row.product },
            { label: 'VID / PID', value: row.vid != null ? `${hex4(row.vid)} / ${hex4(row.pid)}` : null, mono: true },
            { label: 'Interface', value: row.interfaceNumber != null && row.transport === 'usb' ? `${row.interfaceNumber}${row.hintLabel ? ` — ${row.hintLabel}` : ''}` : null },
            { label: 'Serial number', value: row.serialNumber, mono: true, copy: true },
            { label: 'Revision', value: usb?.revision != null ? hex4(usb.revision) : null, mono: true },
            { label: 'USB socket', value: usb?.locationLabel ?? null },
            { label: 'USB path', value: usb?.location, mono: true },
            { label: 'Bluetooth address', value: snap?.bluetooth?.address ?? row.btAddress, mono: true, copy: true },
            { label: 'Bluetooth device', value: snap?.bluetooth?.deviceName },
            { label: 'Bluetooth port', value: snap?.bluetooth?.direction ? (snap.bluetooth.direction === 'outgoing' ? 'Outgoing (connects to the device)' : 'Incoming (waits for connections)') : null },
            { label: 'Service', value: snap?.bluetooth?.service },
            { label: 'Virtual provider', value: snap?.virtualPort ? `${snap.virtualPort.provider}${snap.virtualPort.heuristic ? ' (best guess)' : ''}` : row.virtualProvider },
            { label: 'Virtual pairing', value: snap?.virtualPort?.detail },
          ]}
        />
        {#if snap && !detail?.snapshotLive}
          <div class="muted small">Shown from the last time this device was connected.</div>
        {/if}
      </Section>

      <Section key="history" title="History" badge={detail?.portHistory.length ? `${detail.portHistory.length} port${detail.portHistory.length > 1 ? 's' : ''}` : null}>
        <KV
          items={[
            { label: 'First seen', value: dateTime(row.firstSeen) },
            { label: 'Last seen', value: isConnected(row.status) ? 'Now' : row.lastSeen ? `${dateTime(row.lastSeen)}${row.lastSeenSource === 'os' ? ' (from Windows)' : ''}` : 'Never (by ComInspect)' },
            { label: 'Installed (OS)', value: snap?.osTimes.firstInstall ? dateTime(snap.osTimes.firstInstall) : null },
            { label: 'Last arrival (OS)', value: !isConnected(row.status) && snap?.osTimes.lastArrival ? dateTime(snap.osTimes.lastArrival) : null },
            { label: 'Last removal (OS)', value: snap?.osTimes.lastRemoval ? dateTime(snap.osTimes.lastRemoval) : null },
          ]}
        />
        {#if detail && detail.portHistory.length}
          <div class="subhead">Port assignments on this computer</div>
          <ul class="ports">
            {#each detail.portHistory as p (p.port)}
              <li class:current={p.current}>
                <span class="mono">{p.port}</span>
                <span class="muted">{dateTime(p.firstObserved)} → {p.current ? 'now' : dateTime(p.lastObserved)}</span>
                {#if p.current}<span class="pill accent">current</span>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if detail?.importedFrom}
          <div class="note">
            <Icon name="info" size={13} />
            <div>
              Imported from <strong>{detail.importedFrom.hostname ?? 'another computer'}</strong> ({detail.importedFrom.os}){#if detail.importedFrom.lastPort}, where it was {detail.importedFrom.lastPort}{/if}.
            </div>
          </div>
        {/if}
        {#if detail && detail.events.length}
          <div class="subhead">Recent events</div>
          <ul class="events">
            {#each detail.events.slice(0, 8) as e (e.id)}
              <li>
                <span class="when" title={dateTime(e.ts)}>{relativeTime(e.ts, app.now)}</span>
                <span>{EVENT_TEXT[e.kind] ?? e.kind}{#if e.port} · <span class="mono">{e.port}</span>{/if}{#if e.previousPort} (was <span class="mono">{e.previousPort}</span>){/if}{#if e.detail} <span class="muted">— {e.detail}</span>{/if}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </Section>

      <Section key="recognition" title="How it is recognized">
        {#if detail}
          <p class="identity-note">{detail.identityNote}</p>
          <ul class="keys">
            {#each detail.keys as k (k.kind + k.value)}
              <li>
                <span class="pill {k.strength >= 90 ? 'ok' : k.strength >= 60 ? 'accent' : ''}">{KEY_KIND_LABEL[k.kind] ?? k.kind}</span>
                <span class="mono key">{k.value}</span>
                {#if k.portable}<span class="muted small" title="Also recognized on other computers after an import">portable</span>{/if}
              </li>
            {/each}
          </ul>
        {/if}
      </Section>

      <Section key="system" title="System">
        <KV
          items={[
            { label: isWindows ? 'Instance ID' : 'Device', value: sys?.instanceId, mono: true, copy: true },
            { label: 'Hardware IDs', value: sys?.hardwareIds ?? [], mono: true },
            { label: 'Compatible IDs', value: sys?.compatibleIds ?? [], mono: true },
            { label: 'Parent', value: sys?.parentInstanceId, mono: true },
            { label: 'Class', value: sys?.deviceClass ? `${sys.deviceClass}${sys.classGuid ? ` ${sys.classGuid}` : ''}` : null },
            { label: 'Enumerator', value: sys?.enumerator },
            { label: 'Driver', value: sys?.driver, mono: true },
            { label: 'Driver provider', value: sys?.driverProvider },
            { label: 'Driver version', value: sys?.driverVersion },
            { label: 'Driver date', value: sys?.driverDate },
            { label: 'Driver INF', value: sys?.driverInf, mono: true },
            { label: 'Container ID', value: sys?.containerId, mono: true },
            { label: 'Location', value: sys?.locationInfo },
            { label: 'Location paths', value: sys?.locationPaths ?? [], mono: true },
            { label: 'Device path', value: sys?.devicePath, mono: true },
            ...(snap?.extra ?? []).map((e) => ({ label: e.name, value: e.value, mono: e.value.includes('\\') || e.value.includes('/') })),
          ]}
        />
        {#each snap?.notes ?? [] as n (n)}<div class="note"><Icon name="info" size={13} /><div>{n}</div></div>{/each}
      </Section>

      <Section key="diagnostics" title="Diagnostics">
        <Diagnostics {row} />
      </Section>

      <div class="manage">
        <button
          class="btn small"
          disabled={!detail || detail.mergeCandidates.length === 0}
          title="Link this entry with another one that is the same physical device (for example a cable without a serial number plugged into a different USB socket)."
          onclick={() => (app.dialog = 'merge')}
        >
          <Icon name="link" size={13} /> Same device as…
        </button>
        <button class="btn small" onclick={() => app.setIgnored(row.deviceId, !row.ignored)}>
          <Icon name={row.ignored ? 'eye' : 'eye-off'} size={13} />
          {row.ignored ? 'Show in list' : 'Ignore port'}
        </button>
        <button class="btn small danger" onclick={forget}><Icon name="trash" size={13} /> Forget…</button>
      </div>
    </div>
  </aside>
{:else}
  <aside class="inspector empty">
    <Icon name="info" size={22} />
    <p>Select a port to see everything ComInspect knows about it.</p>
    <p class="muted small">Tip: double-click a row to name it. ↑/↓ moves the selection.</p>
  </aside>
{/if}

<style>
  .inspector {
    width: 350px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  .inspector.empty {
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 24px;
    text-align: center;
    color: var(--text-2);
  }
  .inspector.empty p {
    margin: 0;
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 12px 10px 10px 14px;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .sub .port {
    font-weight: 600;
    color: var(--text);
  }
  .device-line {
    margin-top: 2px;
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: 12px;
  }
  .finding {
    margin: 0 14px 10px;
  }
  .finding :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .callout.danger :global(svg) {
    color: var(--danger);
  }
  .callout.warn :global(svg) {
    color: var(--warn);
  }
  .hints {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hint {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .hint.caution {
    background: var(--warn-soft);
    border-color: transparent;
  }
  .hint-title {
    display: flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
  }
  .hint-detail {
    font-size: 12px;
    color: var(--text-2);
  }
  .small {
    font-size: 11.5px;
  }
  .note {
    display: flex;
    gap: 6px;
    margin-top: 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .note :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--muted);
  }
  .subhead {
    margin: 12px 0 5px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .ports,
  .events,
  .keys {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ports li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .ports li .mono {
    min-width: 56px;
    font-weight: 600;
  }
  .ports li:not(.current) .mono {
    color: var(--text-2);
  }
  .events li {
    display: grid;
    grid-template-columns: 86px 1fr;
    gap: 6px;
    font-size: 12px;
  }
  .when {
    color: var(--muted);
  }
  .identity-note {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .keys li {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .key {
    font-size: 11px;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .manage {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 12px 14px 0;
    border-top: 1px solid var(--border);
  }
</style>
