<script lang="ts">
  import type { Snippet } from 'svelte';
  import { app } from '../lib/app.svelte';
  import Icon from './Icon.svelte';

  let {
    key,
    title,
    badge,
    children,
  }: { key: string; title: string; badge?: string | number | null; children: Snippet } = $props();

  const collapsed = $derived(!!app.collapsed[key]);
</script>

<section class="section">
  <button class="head" onclick={() => app.toggleSection(key)} aria-expanded={!collapsed}>
    <Icon name={collapsed ? 'chevron-right' : 'chevron-down'} size={14} />
    <span>{title}</span>
    {#if badge != null && badge !== ''}<span class="badge">{badge}</span>{/if}
  </button>
  {#if !collapsed}
    <div class="content">
      {@render children()}
    </div>
  {/if}
</section>

<style>
  .section {
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 34px;
    padding: 0 14px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 650;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    text-align: left;
  }
  .head:hover {
    color: var(--text);
  }
  .badge {
    margin-left: auto;
    font-size: 11px;
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
    color: var(--muted);
  }
  .content {
    padding: 0 14px 14px;
  }
</style>
