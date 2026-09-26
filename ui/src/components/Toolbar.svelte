<script lang="ts">
  import { app } from '../lib/app.svelte';
  import Icon from './Icon.svelte';

  let { searchFocus = 0 }: { searchFocus?: number } = $props();

  let menuOpen = $state(false);
  let searchEl: HTMLInputElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (searchFocus > 0) {
      searchEl?.focus();
      searchEl?.select();
    }
  });

  function onWindowClick(e: MouseEvent) {
    if (menuOpen && menuEl && !menuEl.contains(e.target as Node)) menuOpen = false;
  }

  function run(action: () => void) {
    menuOpen = false;
    action();
  }

  const ignoredCount = $derived(app.view?.summary.ignored ?? 0);
</script>

<svelte:window onclick={onWindowClick} />

<header class="toolbar">
  <div class="brand">
    <img src="/favicon.svg" alt="" width="20" height="20" />
    <span>ComInspect</span>
  </div>

  <label class="search">
    <Icon name="search" size={15} />
    <input
      bind:this={searchEl}
      type="search"
      bind:value={app.search}
      placeholder="Search nickname, COM port, radio, VID:PID, serial, notes…"
      aria-label="Search ports"
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          app.search = '';
          searchEl?.blur();
        }
      }}
    />
    {#if app.search}
      <button class="clear" aria-label="Clear search" onclick={() => (app.search = '')}>
        <Icon name="x" size={13} />
      </button>
    {/if}
  </label>

  <div class="actions">
    <button
      class="btn"
      onclick={() => app.refresh()}
      disabled={app.refreshing}
      title="Rescan serial ports (F5). The list also updates automatically."
    >
      <Icon name="refresh" size={15} class={app.refreshing ? 'spin' : ''} />
      Refresh
    </button>
    <button
      class="icon-btn"
      class:active={app.inspectorOpen}
      onclick={() => (app.inspectorOpen = !app.inspectorOpen)}
      title={app.inspectorOpen ? 'Hide details panel' : 'Show details panel'}
      aria-label="Toggle details panel"
    >
      <Icon name="sidebar" />
    </button>
    <div class="menu-wrap" bind:this={menuEl}>
      <button
        class="icon-btn"
        onclick={() => (menuOpen = !menuOpen)}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        aria-label="Menu"
        title={app.updateBadge ? 'Menu — an update is available' : 'Menu'}
      >
        <Icon name="menu" />
        {#if app.updateBadge}<span class="badge" aria-hidden="true"></span>{/if}
      </button>
      {#if menuOpen}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={() => run(() => (app.dialog = 'about'))}>
            <Icon name="info" size={15} /> About &amp; updates
            {#if app.updateBadge}<span class="pill accent">Update available</span>{/if}
          </button>
          <hr />
          <button role="menuitem" onclick={() => run(() => app.exportInventory())}>
            <Icon name="download" size={15} /> Export port mappings…
          </button>
          <button role="menuitem" onclick={() => run(() => (app.dialog = 'import'))}>
            <Icon name="upload" size={15} /> Import port mappings…
          </button>
          <button role="menuitem" onclick={() => run(() => (app.dialog = 'backups'))}>
            <Icon name="database" size={15} /> Database backups…
          </button>
          <hr />
          <button role="menuitemcheckbox" aria-checked={app.showIgnored} onclick={() => run(() => app.setShowIgnored(!app.showIgnored))}>
            <Icon name={app.showIgnored ? 'check' : 'eye-off'} size={15} /> Show ignored ports
            {#if ignoredCount}<span class="count">{ignoredCount}</span>{/if}
          </button>
          <hr />
          <button role="menuitem" onclick={() => run(() => void app.backend?.openLocation('data'))}>
            <Icon name="folder" size={15} /> Open data folder
          </button>
          <button role="menuitem" onclick={() => run(() => void app.backend?.openLocation('logs'))}>
            <Icon name="folder" size={15} /> Open log folder
          </button>
          <button role="menuitem" onclick={() => run(() => void app.backend?.openLink('issues'))}>
            <Icon name="external" size={15} /> Report an issue
          </button>
        </div>
      {/if}
    </div>
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 48px;
    padding: 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 650;
    font-size: 14px;
    letter-spacing: 0.1px;
    min-width: 128px;
  }
  .search {
    flex: 1;
    max-width: 560px;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent);
    box-shadow: var(--focus);
    background: var(--surface);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 26px;
    border: 0;
    padding: 0;
    background: transparent;
    box-shadow: none;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .clear {
    display: flex;
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 2px;
  }
  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .icon-btn.active {
    color: var(--accent);
  }
  .menu-wrap {
    position: relative;
  }
  .badge {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--surface);
  }
  .menu {
    position: absolute;
    right: 0;
    top: 34px;
    z-index: 50;
    min-width: 250px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 30px;
    padding: 0 9px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    text-align: left;
  }
  .menu button:hover {
    background: var(--surface-3);
  }
  .menu button :global(svg) {
    color: var(--muted);
  }
  .menu .pill,
  .menu .count {
    margin-left: auto;
  }
  .count {
    color: var(--muted);
    font-size: 12px;
  }
  .menu hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 2px;
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
