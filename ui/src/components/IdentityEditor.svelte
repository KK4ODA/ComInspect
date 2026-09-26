<script lang="ts">
  import { untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { CATEGORIES, CAT_STATUSES, PURPOSES } from '../lib/labels';
  import type { CatStatus, Category, IdentityPatch, PortRow, Purpose } from '../lib/types';

  let { row }: { row: PortRow } = $props();

  let nickname = $state('');
  let equipment = $state('');
  let notes = $state('');
  let deviceId = $state<number | null>(null);
  let saveState = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  let pending: IdentityPatch = {};
  let timer: ReturnType<typeof setTimeout> | null = null;
  let nicknameEl: HTMLInputElement | undefined = $state();

  // Reset the local fields when a different device is shown.
  $effect.pre(() => {
    const id = row.deviceId;
    if (id !== untrack(() => deviceId)) {
      untrack(() => {
        flush();
        deviceId = id;
        nickname = row.nickname ?? '';
        equipment = row.equipment ?? '';
        notes = row.notes ?? '';
        saveState = 'idle';
      });
    }
  });

  $effect(() => {
    if (app.focusNickname > 0) {
      nicknameEl?.focus();
      nicknameEl?.select();
    }
  });

  async function flush() {
    if (timer) clearTimeout(timer);
    timer = null;
    const id = deviceId;
    const patch = pending;
    pending = {};
    if (id == null || Object.keys(patch).length === 0) return;
    saveState = 'saving';
    const ok = await app.updateIdentity(id, patch);
    saveState = ok ? 'saved' : 'error';
    if (ok) setTimeout(() => saveState === 'saved' && (saveState = 'idle'), 1500);
  }

  function schedule(patch: IdentityPatch, immediate = false) {
    pending = { ...pending, ...patch };
    if (timer) clearTimeout(timer);
    timer = setTimeout(flush, immediate ? 0 : 600);
  }

  function setPurpose(value: string) {
    schedule({ purpose: (value || null) as Purpose | null }, true);
  }

  function setCategory(value: string) {
    schedule({ category: (value || null) as Category | null }, true);
  }

  function setCat(value: CatStatus) {
    schedule({ catStatus: value }, true);
  }
</script>

<div class="editor">
  <label class="field">
    <span>Nickname</span>
    <input
      bind:this={nicknameEl}
      type="text"
      bind:value={nickname}
      maxlength="120"
      placeholder="What is this port? e.g. Main rig CAT, GPS, APRS TNC"
      oninput={() => schedule({ nickname: nickname.trim() || null })}
      onblur={flush}
    />
  </label>
  <label class="field">
    <span>Equipment</span>
    <input
      type="text"
      bind:value={equipment}
      maxlength="120"
      list="equipment-suggestions"
      placeholder="Radio, TNC, interface or cable model"
      oninput={() => schedule({ equipment: equipment.trim() || null })}
      onblur={flush}
    />
    <datalist id="equipment-suggestions">
      {#each app.equipmentSuggestions as item (item)}<option value={item}></option>{/each}
    </datalist>
  </label>
  <div class="row2">
    <label class="field">
      <span>Purpose</span>
      <select value={row.purpose ?? ''} onchange={(e) => setPurpose(e.currentTarget.value)}>
        <option value="">—</option>
        {#each PURPOSES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Category</span>
      <select value={row.category ?? ''} onchange={(e) => setCategory(e.currentTarget.value)}>
        <option value="">—</option>
        {#each CATEGORIES as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
      </select>
    </label>
  </div>
  <div class="field">
    <span>CAT</span>
    <div class="segmented" role="radiogroup" aria-label="CAT status">
      {#each CAT_STATUSES as s (s.value)}
        <button
          role="radio"
          aria-checked={row.catStatus === s.value}
          class:active={row.catStatus === s.value}
          class={s.value}
          onclick={() => setCat(s.value)}>{s.label}</button
        >
      {/each}
    </div>
  </div>
  <label class="field">
    <span>Notes</span>
    <textarea
      rows="3"
      bind:value={notes}
      maxlength="4000"
      placeholder="e.g. which programs use it, baud rate, what RTS/DTR do"
      oninput={() => schedule({ notes: notes.trim() ? notes : null })}
      onblur={flush}
    ></textarea>
  </label>
  <div class="save-state" aria-live="polite">
    {#if saveState === 'saving'}Saving…{:else if saveState === 'saved'}Saved{:else if saveState === 'error'}<span class="err">Not saved</span>{/if}
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .field > span {
    font-size: 11.5px;
    color: var(--muted);
  }
  .field input,
  .field select,
  .field textarea {
    width: 100%;
  }
  .row2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .segmented {
    display: flex;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .segmented button {
    flex: 1;
    height: 26px;
    border: 0;
    border-right: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
  }
  .segmented button:last-child {
    border-right: 0;
  }
  .segmented button.active {
    font-weight: 600;
    color: var(--text);
    background: var(--surface-3);
  }
  .segmented button.verified.active {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .segmented button.not_cat.active {
    background: var(--surface-3);
  }
  .save-state {
    min-height: 16px;
    font-size: 11.5px;
    color: var(--muted);
    text-align: right;
  }
  .err {
    color: var(--danger);
  }
</style>
