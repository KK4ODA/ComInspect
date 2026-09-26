<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/app.svelte';
  import AboutDialog from './components/AboutDialog.svelte';
  import BackupsDialog from './components/BackupsDialog.svelte';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import FilterBar from './components/FilterBar.svelte';
  import Icon from './components/Icon.svelte';
  import ImportDialog from './components/ImportDialog.svelte';
  import Inspector from './components/Inspector.svelte';
  import MergeDialog from './components/MergeDialog.svelte';
  import PortTable from './components/PortTable.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import Toasts from './components/Toasts.svelte';
  import Toolbar from './components/Toolbar.svelte';

  let searchFocus = $state(0);
  let editTick = $state(0);

  onMount(() => {
    void app.init();
    const onError = (e: ErrorEvent) => void app.backend?.log('error', `${e.message} at ${e.filename}:${e.lineno}`);
    const onRejection = (e: PromiseRejectionEvent) => void app.backend?.log('error', `Unhandled: ${String(e.reason)}`);
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);
    return () => {
      app.dispose();
      window.removeEventListener('error', onError);
      window.removeEventListener('unhandledrejection', onRejection);
    };
  });

  function isTyping(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      searchFocus++;
      return;
    }
    if (e.key === 'F5' || (mod && e.key.toLowerCase() === 'r')) {
      e.preventDefault();
      void app.refresh();
      return;
    }
    if (app.dialog || isTyping(e.target)) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      app.moveSelection(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      app.moveSelection(-1);
    } else if (e.key === 'F2') {
      e.preventDefault();
      editTick++;
    } else if (e.key === 'Enter' && app.selectedId != null) {
      e.preventDefault();
      app.focusNickname++;
    } else if (e.key === 'Escape') {
      if (app.filtersActive) app.clearFilters();
      else app.select(null);
    }
  }

  const recovery = $derived(app.appInfo?.startup.recovery ?? null);
  const temporary = $derived(app.view?.storage.temporary ? app.view.storage.reason : null);
  const recovered = $derived(app.appInfo?.recoveredFile ?? null);

  function dismiss(key: string) {
    app.dismissedBanners = [...app.dismissedBanners, key];
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <Toolbar {searchFocus} />

  {#if recovery && !app.dismissedBanners.includes('recovery')}
    <div class="banner warn">
      <Icon name="alert" size={15} />
      <span>
        The previous start of ComInspect {recovery.failedVersion} did not complete.
        {#if recovery.lastGoodVersion}Version {recovery.lastGoodVersion} worked on this computer.{/if}
      </span>
      <button class="btn small" onclick={() => app.backend?.openLocation('logs')}>Open logs</button>
      {#if recovery.lastGoodVersion}
        <button class="btn small" onclick={() => (app.dialog = 'about')}>Reinstall {recovery.lastGoodVersion}…</button>
      {/if}
      <button class="icon-btn" aria-label="Dismiss" onclick={() => dismiss('recovery')}><Icon name="x" size={14} /></button>
    </div>
  {/if}
  {#if temporary}
    <div class="banner danger">
      <Icon name="alert" size={15} />
      <span>{temporary}</span>
      <button class="btn small" onclick={() => (app.dialog = 'backups')}>Backups…</button>
    </div>
  {/if}
  {#if recovered && !app.dismissedBanners.includes('recovered')}
    <div class="banner warn">
      <Icon name="alert" size={15} />
      <span>The device database was damaged and has been replaced by a new one. The damaged file was kept: {recovered}</span>
      <button class="btn small" onclick={() => (app.dialog = 'backups')}>Restore a backup…</button>
      <button class="icon-btn" aria-label="Dismiss" onclick={() => dismiss('recovered')}><Icon name="x" size={14} /></button>
    </div>
  {/if}

  <FilterBar />

  <main>
    {#if app.loading}
      <div class="center muted">Scanning serial ports…</div>
    {:else if app.fatal}
      <div class="center">
        <p><strong>ComInspect could not load the port list.</strong></p>
        <p class="muted">{app.fatal}</p>
        <button class="btn" onclick={() => location.reload()}>Try again</button>
      </div>
    {:else}
      <PortTable {editTick} />
      {#if app.inspectorOpen}<Inspector />{/if}
    {/if}
  </main>

  <StatusBar />
  <Toasts />

  {#if app.dialog === 'about'}<AboutDialog />{/if}
  {#if app.dialog === 'import'}<ImportDialog />{/if}
  {#if app.dialog === 'backups'}<BackupsDialog />{/if}
  {#if app.dialog === 'merge'}<MergeDialog />{/if}
  {#if app.dialog === 'confirm'}<ConfirmDialog />{/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  main {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
  }
  .center p {
    margin: 0;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    font-size: 12.5px;
    border-bottom: 1px solid var(--border);
  }
  .banner span {
    flex: 1;
  }
  .banner.warn {
    background: var(--warn-soft);
  }
  .banner.warn > :global(svg) {
    color: var(--warn);
  }
  .banner.danger {
    background: var(--danger-soft);
  }
  .banner.danger > :global(svg) {
    color: var(--danger);
  }
</style>
