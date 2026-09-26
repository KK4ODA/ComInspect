<script lang="ts">
  import { app } from '../lib/app.svelte';
  import Icon from './Icon.svelte';

  const icon = { info: 'info', success: 'check', warning: 'alert', error: 'alert' } as const;
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}" role="status">
      <span class="ic"><Icon name={icon[t.kind]} size={15} /></span>
      <div class="text">
        <div class="title">{t.title}</div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
      </div>
      {#if t.action}
        <button
          class="btn small"
          onclick={() => {
            t.action?.run();
            app.dismissToast(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      <button class="close" aria-label="Dismiss" onclick={() => app.dismissToast(t.id)}><Icon name="x" size={13} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    bottom: 40px;
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(380px, calc(100vw - 28px));
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 10px 10px 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-left-width: 3px;
    border-radius: 8px;
    box-shadow: var(--shadow);
    pointer-events: auto;
    animation: slide 0.18s ease-out;
  }
  @keyframes slide {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }
  .toast.success {
    border-left-color: var(--ok);
  }
  .toast.info {
    border-left-color: var(--accent);
  }
  .toast.warning {
    border-left-color: var(--warn);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .ic {
    margin-top: 1px;
  }
  .success .ic {
    color: var(--ok);
  }
  .info .ic {
    color: var(--accent);
  }
  .warning .ic {
    color: var(--warn);
  }
  .error .ic {
    color: var(--danger);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .detail {
    margin-top: 1px;
    color: var(--text-2);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .close {
    display: flex;
    padding: 3px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
  }
  .close:hover {
    background: var(--surface-3);
  }
</style>
