<script lang="ts" module>
  export interface KVItem {
    label: string;
    value: string | number | null | undefined | string[];
    mono?: boolean;
    title?: string;
    copy?: boolean;
  }
</script>

<script lang="ts">
  import Icon from './Icon.svelte';

  let { items }: { items: KVItem[] } = $props();

  const visible = $derived(
    items.filter((i) => (Array.isArray(i.value) ? i.value.length > 0 : i.value != null && i.value !== '')),
  );

  let copied = $state<string | null>(null);

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = text;
      setTimeout(() => (copied = null), 1200);
    } catch {
      /* clipboard unavailable */
    }
  }
</script>

<dl class="kv">
  {#each visible as item (item.label)}
    <dt>{item.label}</dt>
    <dd class:mono={item.mono} title={item.title}>
      {#if Array.isArray(item.value)}
        {#each item.value as line, i (i)}<div class="line">{line}</div>{/each}
      {:else}
        <span class="val">{item.value}</span>
      {/if}
      {#if item.copy && !Array.isArray(item.value)}
        <button class="copy" title="Copy" aria-label={`Copy ${item.label}`} onclick={() => copy(String(item.value))}>
          <Icon name={copied === String(item.value) ? 'check' : 'copy'} size={12} />
        </button>
      {/if}
    </dd>
  {/each}
</dl>

<style>
  .kv {
    display: grid;
    grid-template-columns: minmax(96px, 34%) 1fr;
    gap: 5px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
    font-size: 12px;
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
    display: flex;
    align-items: flex-start;
    gap: 4px;
  }
  dd:has(.line) {
    display: block;
  }
  .line + .line {
    margin-top: 2px;
  }
  .val {
    min-width: 0;
  }
  .copy {
    flex: none;
    display: inline-flex;
    padding: 2px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    opacity: 0;
  }
  dd:hover .copy {
    opacity: 1;
  }
  .copy:hover {
    background: var(--surface-3);
    color: var(--text);
  }
</style>
