<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { errorMessage } from '../lib/api';
  import { isConnected } from '../lib/filters';
  import type {
    CatProtocol,
    CatQueryReport,
    ControlLine,
    OpenTestReport,
    PortRow,
    ProbeCatalog,
    PttTestReport,
  } from '../lib/types';
  import Icon from './Icon.svelte';

  let { row }: { row: PortRow } = $props();

  const BAUDS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];

  /** Picks a starting protocol and CI-V address from what the device itself
   * reports (USB serial string, hints) or what the user entered. Nothing here
   * assumes a particular radio; the user can change every field. */
  function suggestedDefaults(): { protocol: CatProtocol; civ: string } {
    const text = [
      row.equipment,
      row.nickname,
      row.hintLabel,
      row.serialNumber,
      ...(app.detail?.row.deviceId === row.deviceId ? app.detail.hints.map((h) => h.suggestedEquipment ?? h.title) : []),
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase();
    if (/\bicom\b|\bic-?\d|xiegu|ci-v/.test(text)) {
      const model = /ic-?\s?(\d{2,4}[a-z]*)/.exec(text)?.[1];
      const match = model
        ? catalog?.icomAddresses.find(([, name]) => name.toLowerCase().replace('ic-', '') === model)
        : undefined;
      return { protocol: 'icom_id', civ: match ? match[0].toString(16).toUpperCase().padStart(2, '0') : '' };
    }
    if (/ft-?(817|818|857|897)/.test(text)) return { protocol: 'yaesu_legacy_frequency', civ: '' };
    return { protocol: 'kenwood_id', civ: '' };
  }

  let catalog = $state<ProbeCatalog | null>(null);
  let busy = $state<'open' | 'cat' | 'ptt' | null>(null);
  let openResult = $state<OpenTestReport | null>(null);
  let catResult = $state<CatQueryReport | null>(null);
  let pttResult = $state<PttTestReport | null>(null);
  let failure = $state<string | null>(null);

  let protocol = $state<CatProtocol>('kenwood_id');
  let baud = $state(0);
  let stopBits = $state(1);
  let civ = $state('');
  let pttLine = $state<ControlLine>('rts');
  let pttMs = $state(1000);

  const port = $derived(row.port ?? '');
  const usable = $derived(isConnected(row.status) && !!row.port);
  const isIcom = $derived(protocol === 'icom_id' || protocol === 'icom_frequency');

  $effect(() => {
    void row.deviceId;
    openResult = null;
    catResult = null;
    pttResult = null;
    failure = null;
  });

  // Choose sensible defaults per device once the probe catalog is loaded.
  let defaultsFor = $state<number | null>(null);
  $effect(() => {
    if (catalog && defaultsFor !== row.deviceId) {
      defaultsFor = row.deviceId;
      const d = suggestedDefaults();
      chooseProbe(d.protocol);
      civ = d.civ;
    }
  });

  $effect(() => {
    if (!catalog && app.backend) {
      app.backend.probeCatalog().then((c) => (catalog = c)).catch(() => undefined);
    }
  });

  function chooseProbe(value: CatProtocol) {
    protocol = value;
    const probe = catalog?.probes.find((p) => p.protocol === value);
    if (probe) stopBits = probe.defaultStopBits;
  }

  async function run<T>(kind: 'open' | 'cat' | 'ptt', fn: () => Promise<T>): Promise<T | null> {
    busy = kind;
    failure = null;
    try {
      return await fn();
    } catch (e) {
      failure = errorMessage(e);
      return null;
    } finally {
      busy = null;
    }
  }

  async function openTest() {
    openResult = await run('open', () => app.backend!.openTest(port));
  }

  async function catQuery() {
    const address = civ.trim() ? parseInt(civ, 16) : 0;
    catResult = await run('cat', () =>
      app.backend!.catQuery(
        port,
        { baudRate: baud, dataBits: 8, parity: 'none', stopBits, flowControl: 'none' },
        protocol,
        Number.isFinite(address) ? address & 0xff : 0,
        1200,
      ),
    );
  }

  function confirmPtt() {
    const seconds = (pttMs / 1000).toFixed(1);
    app.confirm({
      title: 'Key the transmitter?',
      message: `ComInspect will assert ${pttLine.toUpperCase()} on ${port} for ${seconds} s and then release it. If this port controls PTT through ${pttLine.toUpperCase()}, your radio WILL TRANSMIT. Make sure an antenna or dummy load is connected and that you may transmit on the current frequency.`,
      confirmLabel: `Transmit for ${seconds} s`,
      danger: true,
      onConfirm: async () => {
        pttResult = await run('ptt', () => app.backend!.pttTest(port, pttLine, pttMs));
      },
    });
  }

  async function markVerified() {
    const patch = row.purpose ? { catStatus: 'verified' as const } : { catStatus: 'verified' as const, purpose: 'cat' as const };
    if (await app.updateIdentity(row.deviceId, patch)) app.toast('success', 'Marked as a verified CAT port');
  }

  const outcomeClass = (o: string) => (o === 'opened' ? 'ok' : o === 'in_use' ? 'warn' : 'danger');
  const outcomeText: Record<string, string> = {
    opened: 'Opened',
    in_use: 'In use',
    permission_denied: 'Permission denied',
    not_found: 'Not found',
    error: 'Error',
  };
  const line = (v: boolean | null | undefined) => (v == null ? '—' : v ? 'ON' : 'off');
</script>

<div class="diag">
  <div class="callout warn">
    <Icon name="alert" size={15} />
    <div>
      These tests open the port. Opening a serial port can change the <b>DTR</b> and <b>RTS</b> lines, which many stations
      use for PTT or CW keying. ComInspect releases both lines immediately, but the operating system may pulse them
      briefly. Nothing runs unless you click a button.
    </div>
  </div>

  {#if !usable}
    <p class="muted">Diagnostics are available while the device is connected.</p>
  {:else}
    <div class="block">
      <div class="title">Port availability</div>
      <p class="muted">Checks whether the port can be opened or is in use by another program. No data is sent.</p>
      <button class="btn small" onclick={openTest} disabled={busy !== null}>
        <Icon name="plug" size={13} />
        {busy === 'open' ? 'Testing…' : 'Test open'}
      </button>
      {#if openResult}
        <div class="result">
          <span class="pill {outcomeClass(openResult.outcome)}">{outcomeText[openResult.outcome]}</span>
          <span>{openResult.message}</span>
          {#if openResult.lines}
            <div class="lines mono">
              CTS {line(openResult.lines.cts)} · DSR {line(openResult.lines.dsr)} · DCD {line(openResult.lines.dcd)} · RI {line(openResult.lines.ri)}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="block">
      <div class="title">CAT query (read-only)</div>
      <p class="muted">
        Sends one read-only command and shows the reply. Use it to find out which port is the CAT port. With baud set to
        Auto, common rates are tried in turn.
      </p>
      <label class="field">
        <span>Command</span>
        <select value={protocol} onchange={(e) => chooseProbe(e.currentTarget.value as CatProtocol)}>
          {#each catalog?.probes ?? [] as p (p.protocol)}<option value={p.protocol}>{p.label}</option>{/each}
        </select>
      </label>
      <div class="grid3">
        <label class="field">
          <span>Baud</span>
          <select bind:value={baud}>
            <option value={0}>Auto</option>
            {#each BAUDS as b (b)}<option value={b}>{b}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span>Stop bits</span>
          <select bind:value={stopBits}>
            <option value={1}>1</option>
            <option value={2}>2</option>
          </select>
        </label>
        {#if isIcom}
          <label class="field">
            <span>CI-V address</span>
            <input
              type="text"
              bind:value={civ}
              list="civ-addresses"
              maxlength="2"
              class="mono"
              placeholder="00"
              title="The radio's CI-V address in hex (see the radio's CI-V menu). Blank sends to address 00, which many radios answer."
            />
            <datalist id="civ-addresses">
              {#each catalog?.icomAddresses ?? [] as [addr, model] (addr)}
                <option value={addr.toString(16).toUpperCase().padStart(2, '0')}>{model}</option>
              {/each}
            </datalist>
          </label>
        {/if}
      </div>
      <button class="btn small" onclick={catQuery} disabled={busy !== null}>
        <Icon name="radio" size={13} />
        {busy === 'cat' ? (baud === 0 ? 'Trying baud rates…' : 'Waiting for reply…') : 'Send query'}
      </button>
      {#if catResult}
        <div class="result">
          <span class="pill {catResult.recognized ? 'ok' : catResult.outcome === 'opened' ? 'warn' : 'danger'}">
            {catResult.recognized ? 'Reply received' : catResult.outcome === 'opened' ? 'No valid reply' : outcomeText[catResult.outcome]}
          </span>
          <span>{catResult.summary}</span>
          {#if catResult.recognized && baud === 0}
            <button class="link" onclick={() => (baud = catResult?.baudRate ?? 0)}>Use {catResult.baudRate} baud for the next query</button>
          {/if}
          <div class="hex mono">
            <div><span class="muted">Sent</span> {catResult.sentHex}</div>
            <div><span class="muted">Received</span> {catResult.receivedHex || '—'}</div>
            {#if catResult.receivedText}<div><span class="muted">Text</span> {catResult.receivedText}</div>{/if}
          </div>
          {#if catResult.recognized && row.catStatus !== 'verified'}
            <button class="btn small" onclick={markVerified}><Icon name="check" size={13} /> Mark as Verified CAT</button>
          {/if}
        </div>
      {/if}
    </div>

    <div class="block">
      <div class="title">PTT test</div>
      <p class="muted">Asserts RTS or DTR briefly to find the port and line that key your radio. Requires confirmation.</p>
      <div class="grid3">
        <label class="field">
          <span>Line</span>
          <select bind:value={pttLine}>
            <option value="rts">RTS</option>
            <option value="dtr">DTR</option>
          </select>
        </label>
        <label class="field">
          <span>Duration</span>
          <select bind:value={pttMs}>
            <option value={500}>0.5 s</option>
            <option value={1000}>1 s</option>
            <option value={2000}>2 s</option>
          </select>
        </label>
      </div>
      <button class="btn small danger" onclick={confirmPtt} disabled={busy !== null}>
        <Icon name="zap" size={13} />
        {busy === 'ptt' ? 'Transmitting…' : 'Key transmitter…'}
      </button>
      {#if pttResult}
        <div class="result">
          <span class="pill {outcomeClass(pttResult.outcome)}">{outcomeText[pttResult.outcome]}</span>
          <span>{pttResult.message}</span>
        </div>
      {/if}
    </div>
  {/if}
  {#if failure}<div class="callout danger">{failure}</div>{/if}
</div>

<style>
  .diag {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .callout :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--warn);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-start;
  }
  .title {
    font-weight: 600;
  }
  .block p {
    margin: 0;
    font-size: 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
  }
  .field > span {
    font-size: 11.5px;
    color: var(--muted);
  }
  .field select,
  .field input {
    width: 100%;
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    width: 100%;
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 5px;
    align-items: flex-start;
    width: 100%;
    padding: 8px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12px;
  }
  .hex {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-wrap: anywhere;
    font-size: 11.5px;
  }
  .hex .muted {
    display: inline-block;
    width: 64px;
    font-family: var(--font);
  }
  .lines {
    font-size: 11.5px;
  }
  .link {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--accent);
    font-size: 12px;
  }
</style>
