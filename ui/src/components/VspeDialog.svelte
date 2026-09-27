<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { dateTime } from '../lib/format';
  import { baseName, describeDevice, portList } from '../lib/vspe';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  const view = $derived(app.vspe);
  const mode = $derived(view?.source.mode ?? 'autostart');
  const chosen = $derived(view && view.source.mode !== 'autostart' ? view.source.path : null);

  function close() {
    app.dialog = null;
  }
</script>

<Modal title="VSPE configuration" width={580} onclose={close}>
  <p>
    ComInspect reads a configuration file of Eterlogic's Virtual Serial Ports Emulator (VSPE) to show how VSPE connects
    your ports. It only reads the file and never changes VSPE. Choose where to read it from:
  </p>

  <div class="choices" role="radiogroup" aria-label="Where to read the VSPE configuration">
    <button class="choice" class:on={mode === 'autostart'} role="radio" aria-checked={mode === 'autostart'} onclick={() => app.useVspeAutostart()}>
      <span class="radio"></span>
      <span>
        <strong>VSPE's startup configuration</strong>
        <span class="muted">
          The file VSPE's service loads when Windows starts. Update it in VSPE with <em>File → Save as autostart config</em>.
          {#if view?.autostartPath}<span class="path mono">{view.autostartPath}</span>{/if}
        </span>
      </span>
    </button>
    <button class="choice" class:on={mode === 'folder'} role="radio" aria-checked={mode === 'folder'} onclick={() => app.chooseVspeFolder()}>
      <span class="radio"></span>
      <span>
        <strong>The newest file in a folder…</strong>
        <span class="muted">
          For when you save a new .vspe file for every change: ComInspect uses whichever you saved last.
          {#if mode === 'folder' && chosen}<span class="path mono">{chosen}</span>{/if}
        </span>
      </span>
    </button>
    <button class="choice" class:on={mode === 'file'} role="radio" aria-checked={mode === 'file'} onclick={() => app.chooseVspeFile()}>
      <span class="radio"></span>
      <span>
        <strong>A specific file…</strong>
        <span class="muted">
          Always the same file.
          {#if mode === 'file' && chosen}<span class="path mono">{chosen}</span>{/if}
        </span>
      </span>
    </button>
  </div>

  <div class="status">
    {#if view?.error}
      <div class="callout warn"><Icon name="alert" size={15} /><div>{view.error}</div></div>
    {:else if view?.file}
      <div class="reading">
        <strong title={view.file}>{baseName(view.file)}</strong>
        <span class="muted">
          {#if view.modifiedAt}saved {dateTime(view.modifiedAt)} ·{/if}
          {view.devices.length} {view.devices.length === 1 ? 'device' : 'devices'}
        </span>
      </div>
      {#if view.devices.length}
        <ul class="devices">
          {#each view.devices as d, i (i)}<li class:unknown={!d.layout}>{describeDevice(d)}</li>{/each}
        </ul>
      {/if}
      {#if app.vspeMissing.length}
        <div class="callout warn">
          <Icon name="alert" size={15} />
          <div>
            VSPE ports on this computer that aren't in this file: <strong>{portList(app.vspeMissing)}</strong>. If you changed
            VSPE's setup since the file was saved, save it again in VSPE, or pick the newer file above.
          </div>
        </div>
      {/if}
    {:else}
      <p class="muted">VSPE's startup configuration wasn't found on this computer.</p>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn primary" onclick={close}>Done</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 14px;
  }
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
  }
  .choice:hover {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }
  .choice.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .choice > span:last-child {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .choice .muted {
    font-size: 12px;
    line-height: 1.45;
  }
  .radio {
    flex: none;
    width: 14px;
    height: 14px;
    margin-top: 2px;
    border: 1.5px solid var(--border-strong);
    border-radius: 50%;
  }
  .choice.on .radio {
    border: 4px solid var(--accent);
  }
  .path {
    display: block;
    margin-top: 2px;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .reading {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  .reading .muted {
    font-size: 12px;
  }
  .devices {
    margin: 0;
    padding-left: 18px;
    font-size: 12.5px;
    line-height: 1.6;
  }
  .devices li.unknown {
    color: var(--muted);
  }
  .callout :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--warn);
  }
</style>
