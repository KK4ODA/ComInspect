<script lang="ts">
  import { app } from '../lib/app.svelte';
  import Modal from './Modal.svelte';

  const request = $derived(app.confirmRequest);
  let working = $state(false);

  function close() {
    app.dialog = null;
    app.confirmRequest = null;
  }

  async function confirm() {
    if (!request) return;
    working = true;
    const action = request.onConfirm;
    close();
    try {
      await action();
    } finally {
      working = false;
    }
  }
</script>

{#if request}
  <Modal title={request.title} width={440} onclose={close}>
    <p class="message">{request.message}</p>
    {#snippet footer()}
      <button class="btn" onclick={close} data-autofocus>Cancel</button>
      <button class="btn {request.danger ? 'danger solid' : 'primary'}" onclick={confirm} disabled={working}>
        {request.confirmLabel}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .message {
    margin: 0;
    line-height: 1.5;
  }
</style>
