<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { errorMessage } from '../lib/api';
  import { bytes, dateTime } from '../lib/format';
  import type { BackupInfo } from '../lib/types';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  let backups = $state<BackupInfo[] | null>(null);
  let working = $state(false);
  let message = $state<string | null>(null);

  async function load() {
    try {
      backups = (await app.backend?.listBackups()) ?? [];
    } catch (e) {
      message = errorMessage(e);
      backups = [];
    }
  }

  $effect(() => {
    void load();
  });

  function close() {
    app.dialog = null;
  }

  async function create() {
    working = true;
    try {
      await app.backend?.createBackup();
      await load();
      message = 'Backup created.';
    } catch (e) {
      message = errorMessage(e);
    } finally {
      working = false;
    }
  }

  function restore(b: BackupInfo) {
    app.confirm({
      title: 'Restore this backup?',
      message: `The current device database will be replaced by the backup from ${dateTime(b.modified)}. Names, notes and history changed since then will be lost. The current database is kept next to the original as a safety copy.`,
      confirmLabel: 'Restore backup',
      danger: true,
      onConfirm: async () => {
        try {
          const view = await app.backend?.restoreBackup(b.fileName);
          if (view) app.applyView(view);
          app.toast('success', 'Backup restored');
        } catch (e) {
          app.error('Restore failed', e);
        }
      },
    });
  }
</script>

<Modal title="Database backups" width={600} onclose={close}>
  <p>
    ComInspect backs up its device database automatically before upgrades, schema migrations and imports. The ten most
    recent backups are kept.
  </p>
  {#if backups === null}
    <p class="muted">Loading…</p>
  {:else if backups.length === 0}
    <p class="muted">No backups yet.</p>
  {:else}
    <table>
      <thead><tr><th>Backup</th><th>Date</th><th>Size</th><th></th></tr></thead>
      <tbody>
        {#each backups as b (b.fileName)}
          <tr>
            <td class="mono name" title={b.path}>{b.fileName}</td>
            <td>{dateTime(b.modified)}</td>
            <td>{bytes(b.size)}</td>
            <td><button class="btn small" onclick={() => restore(b)}>Restore…</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if message}<p class="muted msg">{message}</p>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => void app.backend?.openLocation('backups')}><Icon name="folder" size={14} /> Open folder</button>
    <button class="btn" disabled={working} onclick={create}><Icon name="database" size={14} /> Back up now</button>
    <button class="btn primary" onclick={close}>Done</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 12px;
    line-height: 1.5;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    color: var(--muted);
    font-weight: 600;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--border);
  }
  .name {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .msg {
    margin-top: 10px;
  }
</style>
