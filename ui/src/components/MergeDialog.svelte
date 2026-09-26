<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { relativeTime } from '../lib/format';
  import { STATUS_SHORT } from '../lib/labels';
  import Modal from './Modal.svelte';

  const detail = $derived(app.detail);
  let chosen = $state<number | null>(null);
  let working = $state(false);

  function close() {
    app.dialog = null;
  }

  async function link() {
    if (!detail || chosen == null) return;
    working = true;
    await app.merge(detail.row.deviceId, chosen);
    working = false;
    close();
  }
</script>

{#if detail}
  <Modal title="Same device as…" width={560} onclose={close}>
    <p>
      Link <strong>{detail.row.nickname ?? detail.row.deviceLabel}</strong> with another entry that is the same physical
      device. Use this when a cable without a unique serial number was plugged into a different USB socket and shows up
      twice. The entries are combined; the name and notes of this entry are kept, and empty fields are filled from the
      other one.
    </p>
    {#if detail.mergeCandidates.length === 0}
      <p class="muted">There are no compatible entries.</p>
    {:else}
      <div class="list" role="radiogroup">
        {#each detail.mergeCandidates as c (c.deviceId)}
          <label class="cand" class:selected={chosen === c.deviceId}>
            <input type="radio" name="merge" value={c.deviceId} bind:group={chosen} />
            <span class="main">
              <span class="label">{c.label}</span>
              <span class="muted">{c.port ?? '—'} · {STATUS_SHORT[c.status]} · last seen {relativeTime(c.lastSeen, app.now).toLowerCase()} · {c.reason}</span>
            </span>
          </label>
        {/each}
      </div>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={chosen == null || working} onclick={link}>Link entries</button>
    {/snippet}
  </Modal>
{/if}

<style>
  p {
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .cand.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-weight: 600;
  }
  .main .muted {
    font-size: 12px;
  }
</style>
