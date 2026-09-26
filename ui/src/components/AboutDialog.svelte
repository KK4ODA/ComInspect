<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { errorMessage } from '../lib/api';
  import { bytes, dateTime, relativeTime } from '../lib/format';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  const info = $derived(app.appInfo);
  const u = $derived(app.update);
  let advanced = $state(false);
  let rollbackError = $state<string | null>(null);

  $effect(() => {
    // Refresh counts and paths whenever the dialog opens.
    void app.backend?.getAppInfo().then((i) => (app.appInfo = i)).catch(() => undefined);
  });

  function close() {
    app.dialog = null;
  }

  const busy = $derived(u?.phase === 'checking' || u?.phase === 'downloading' || u?.phase === 'installing');
  const percent = $derived(u?.total ? Math.min(100, Math.round((u.downloaded / u.total) * 100)) : null);
  const releaseDate = $derived(u?.available?.date ? dateTime(Date.parse(u.available.date)) : null);

  async function reinstall(version: string) {
    rollbackError = null;
    app.confirm({
      title: `Reinstall ComInspect ${version}?`,
      message: `ComInspect ${version} will be downloaded from the official releases, verified and installed in place of this version. Your devices, names and settings are kept.`,
      confirmLabel: `Reinstall ${version}`,
      onConfirm: async () => {
        try {
          await app.backend?.reinstallVersion(version, () => undefined);
        } catch (e) {
          rollbackError = errorMessage(e);
        }
      },
    });
  }
</script>

<Modal title="About ComInspect" width={600} onclose={close}>
  <div class="hero">
    <img src="/favicon.svg" alt="" width="52" height="52" />
    <div>
      <div class="name">ComInspect</div>
      <div class="muted">Serial port inventory and identity manager for amateur radio</div>
      {#if info}<div class="muted small">Version {info.version} · {info.platform} {info.arch}</div>{/if}
    </div>
  </div>

  <section>
    <h3>Updates</h3>
    {#if u}
      {#if !u.configured}
        <div class="callout">This build has no update signing key, so automatic updates are disabled. Official releases include it.</div>
      {:else if !u.supported}
        <div class="callout">
          This copy of ComInspect was not installed with the ComInspect installer (for example a development build), so it
          cannot update itself. Download the installer from the releases page.
        </div>
      {/if}

      <div class="versions">
        <div><span class="muted">Installed</span><strong>{u.currentVersion}</strong></div>
        <div>
          <span class="muted">Latest</span>
          <strong>
            {#if u.available}{u.available.version}{:else if u.phase === 'up_to_date'}{u.currentVersion}{:else}—{/if}
          </strong>
        </div>
        {#if releaseDate}<div><span class="muted">Released</span><strong>{releaseDate}</strong></div>{/if}
      </div>

      {#if u.phase === 'up_to_date'}
        <div class="callout ok"><Icon name="check" size={15} /> You're up to date.</div>
      {:else if u.phase === 'error' && u.error}
        <div class="callout warn">{u.error}</div>
      {/if}

      {#if u.available && (u.phase === 'available' || u.phase === 'downloading' || u.phase === 'installing' || u.phase === 'ready_to_restart')}
        <div class="notes">
          <div class="notes-title">What's new in {u.available.version}</div>
          <pre>{u.available.notes || 'See the release page for details.'}</pre>
          <button class="link" onclick={() => app.backend?.openLink('releases', u.available?.version)}>Full release notes <Icon name="external" size={12} /></button>
        </div>
      {/if}

      {#if u.phase === 'downloading' || u.phase === 'installing'}
        <div class="progress" role="progressbar" aria-valuenow={percent ?? undefined} aria-valuemin="0" aria-valuemax="100">
          <div class="bar" style={`width: ${percent ?? 100}%`} class:indeterminate={percent == null}></div>
        </div>
        <div class="muted small">
          {u.phase === 'installing' ? 'Verifying signature and installing…' : `Downloading… ${bytes(u.downloaded)}${u.total ? ` of ${bytes(u.total)}` : ''}`}
        </div>
      {/if}

      <div class="buttons">
        {#if u.phase === 'ready_to_restart'}
          <button class="btn primary" onclick={() => app.backend?.restartApp()}><Icon name="refresh" size={14} /> Restart now</button>
        {:else if u.available && u.phase === 'available'}
          <button class="btn primary" onclick={() => app.installUpdate()} disabled={!u.supported}>
            <Icon name="download" size={14} /> Download and install update
          </button>
        {/if}
        <button class="btn" onclick={() => app.checkForUpdates()} disabled={busy || !u.configured}>
          <Icon name="refresh" size={14} class={u.phase === 'checking' ? 'spin' : ''} />
          {u.phase === 'checking' ? 'Checking…' : 'Check for updates'}
        </button>
        {#if u.lastChecked}<span class="muted small">Last checked {relativeTime(u.lastChecked, app.now).toLowerCase()}</span>{/if}
      </div>

      <label class="check">
        <input type="checkbox" checked={u.autoCheck} onchange={(e) => app.backend?.setAutoUpdateCheck(e.currentTarget.checked).then((s) => (app.update = s))} />
        Check for updates automatically (once a day, in the background)
      </label>

      <button class="link" onclick={() => (advanced = !advanced)}>
        <Icon name={advanced ? 'chevron-down' : 'chevron-right'} size={13} /> Advanced
      </button>
      {#if advanced}
        <div class="advanced">
          <label class="field">
            <span>Update channel</span>
            <select value={u.channel} onchange={(e) => app.backend?.setUpdateChannel(e.currentTarget.value as 'stable' | 'beta').then((s) => (app.update = s))}>
              <option value="stable">Stable (recommended)</option>
              <option value="beta">Beta — pre-release builds for testing</option>
            </select>
          </label>
          {#if u.channel === 'beta'}
            <div class="callout warn">Beta builds may contain bugs. You can switch back to Stable at any time; you will move to the next stable release when it is newer.</div>
          {/if}
          {#if info?.startup.previousVersion}
            <div>
              <button class="btn small" onclick={() => reinstall(info.startup.previousVersion!)} disabled={!u.supported || !u.configured}>
                Reinstall previous version ({info.startup.previousVersion})
              </button>
              {#if rollbackError}<div class="callout warn">{rollbackError}</div>{/if}
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  </section>

  {#if info}
    <section>
      <h3>Your data</h3>
      <p class="muted small">
        Device names, history and settings are stored locally on this computer and are never uploaded. Updates never
        modify them; a backup is taken before every upgrade.
      </p>
      <dl class="kv">
        <dt>Database</dt>
        <dd class="mono">{info.dbPath}</dd>
        <dt>Contents</dt>
        <dd>{info.deviceCount} devices · {info.eventCount} events · schema {info.schemaVersion}</dd>
        <dt>Logs</dt>
        <dd class="mono">{info.logDir}</dd>
      </dl>
      <div class="buttons">
        <button class="btn small" onclick={() => app.backend?.openLocation('data')}><Icon name="folder" size={13} /> Data folder</button>
        <button class="btn small" onclick={() => app.backend?.openLocation('logs')}><Icon name="folder" size={13} /> Logs</button>
        <button class="btn small" onclick={() => (app.dialog = 'backups')}><Icon name="database" size={13} /> Backups…</button>
      </div>
    </section>
  {/if}

  <section>
    <h3>Privacy</h3>
    <p class="muted small">
      ComInspect works fully offline. It has no accounts and no telemetry. The only network request is the optional update
      check, which downloads a small file from GitHub.
    </p>
    <div class="buttons">
      <button class="btn small" onclick={() => app.backend?.openLink('homepage')}><Icon name="external" size={13} /> Project page</button>
      <button class="btn small" onclick={() => app.backend?.openLink('releases')}><Icon name="external" size={13} /> Releases</button>
      <button class="btn small" onclick={() => app.backend?.openLink('issues')}><Icon name="external" size={13} /> Report an issue</button>
    </div>
  </section>
</Modal>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 8px;
  }
  .name {
    font-size: 18px;
    font-weight: 700;
  }
  .small {
    font-size: 12px;
  }
  section {
    padding-top: 12px;
    margin-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
  }
  h3 {
    margin: 0;
    font-size: 13px;
  }
  section p {
    margin: 0;
  }
  .versions {
    display: flex;
    gap: 28px;
  }
  .versions div {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .versions strong {
    font-size: 15px;
  }
  .versions .muted {
    font-size: 11.5px;
  }
  .callout {
    width: 100%;
  }
  .callout.ok :global(svg) {
    color: var(--ok);
  }
  .notes {
    width: 100%;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  .notes-title {
    font-weight: 600;
    margin-bottom: 4px;
  }
  pre {
    margin: 0 0 6px;
    white-space: pre-wrap;
    font: 12.5px/1.5 var(--font);
    max-height: 160px;
    overflow: auto;
  }
  .progress {
    width: 100%;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .bar.indeterminate {
    animation: indet 1.2s ease-in-out infinite;
    width: 30% !important;
  }
  @keyframes indet {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(350%);
    }
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--accent);
    font-size: 12px;
  }
  .advanced {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 320px;
  }
  .field span {
    font-size: 11.5px;
    color: var(--muted);
  }
  .kv {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 4px 10px;
    margin: 0;
    font-size: 12px;
    width: 100%;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
