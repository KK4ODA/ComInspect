<script lang="ts">
  import { app } from '../lib/app.svelte';
  import type { ImportMode } from '../lib/types';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  let working = $state(false);

  function close() {
    app.dialog = null;
  }

  async function run(mode: ImportMode) {
    working = true;
    const imported = await app.importInventory(mode);
    working = false;
    if (imported) close();
  }
</script>

<Modal title="Import port mappings" width={540} onclose={close}>
  <p>
    Import a <span class="mono">serial-port-inventory.json</span> file exported from ComInspect, for example from your
    other shack computer. Devices are recognized by their hardware identity (USB serial number, Bluetooth address), so the
    names follow the radios <strong>whatever COM numbers this computer assigns</strong>.
  </p>
  <ul>
    <li>Radios that are not plugged in yet are remembered and named as soon as they appear.</li>
    <li>COM numbers from the other computer are shown for reference only; they are never applied here.</li>
    <li>Cables without a unique serial number can only be matched on the same computer.</li>
    <li>A database backup is taken before importing.</li>
  </ul>
  <div class="choices">
    <button class="choice" disabled={working} onclick={() => run('merge')} data-autofocus>
      <Icon name="upload" />
      <span>
        <strong>Merge</strong>
        <span class="muted">Keep names and notes already set on this computer; only fill in empty fields.</span>
      </span>
    </button>
    <button class="choice" disabled={working} onclick={() => run('overwrite')}>
      <Icon name="upload" />
      <span>
        <strong>Replace</strong>
        <span class="muted">Use the names, purposes and notes from the file where it has them.</span>
      </span>
    </button>
  </div>
</Modal>

<style>
  p {
    margin: 0 0 10px;
    line-height: 1.5;
  }
  ul {
    margin: 0 0 14px;
    padding-left: 18px;
    color: var(--text-2);
    line-height: 1.55;
  }
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .choice {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    text-align: left;
  }
  .choice:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .choice span {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .choice .muted {
    font-size: 12px;
  }
  .choice :global(svg) {
    flex: none;
    color: var(--accent);
  }
</style>
