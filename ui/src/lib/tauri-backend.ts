import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { Backend } from './api';
import type {
  DownloadEvent,
  InventoryEvent,
  InventoryView,
  ReleasedEvent,
  UpdateStatus,
  UsageView,
} from './types';

function channel(onEvent: (e: DownloadEvent) => void): Channel<DownloadEvent> {
  const ch = new Channel<DownloadEvent>();
  ch.onmessage = onEvent;
  return ch;
}

export function createTauriBackend(): Backend {
  return {
    kind: 'tauri',
    getInventory: () => invoke('get_inventory'),
    refresh: () => invoke('refresh'),
    getDeviceDetail: (deviceId) => invoke('get_device_detail', { deviceId }),
    updateIdentity: (deviceId, patch) => invoke('update_identity', { deviceId, patch }),
    setIgnored: (deviceId, ignored) => invoke('set_ignored', { deviceId, ignored }),
    mergeDevices: (targetId, sourceId) => invoke('merge_devices', { targetId, sourceId }),
    forgetDevice: (deviceId) => invoke('forget_device', { deviceId }),
    exportInventory: () => invoke('export_inventory'),
    importInventory: (mode) => invoke('import_inventory', { mode }),
    getAppInfo: () => invoke('get_app_info'),
    appReady: () => invoke('app_ready'),
    getUiPrefs: () => invoke('get_ui_prefs'),
    setUiPrefs: (prefs) => invoke('set_ui_prefs', { prefs }),
    log: (level, message) => invoke('log_from_ui', { level, message }),
    openLocation: (which) => invoke('open_location', { which }),
    openLink: (which, version) => invoke('open_link', { which, version: version ?? null }),
    getUpdateStatus: () => invoke('get_update_status'),
    checkForUpdates: () => invoke('check_for_updates'),
    installUpdate: (onEvent) => invoke('install_update', { onEvent: channel(onEvent) }),
    reinstallVersion: (version, onEvent) =>
      invoke('reinstall_version', { version, onEvent: channel(onEvent) }),
    setAutoUpdateCheck: (enabled) => invoke('set_auto_update_check', { enabled }),
    setUpdateChannel: (updateChannel) => invoke('set_update_channel', { channel: updateChannel }),
    restartApp: () => invoke('restart_app'),
    listBackups: () => invoke('list_database_backups'),
    createBackup: () => invoke('create_database_backup'),
    restoreBackup: (fileName) => invoke('restore_database_backup', { fileName }),
    probeCatalog: () => invoke('diag_probe_catalog'),
    openTest: (port) => invoke('diag_open_test', { port }),
    catQuery: (port, settings, protocol, civAddress, timeoutMs) =>
      invoke('diag_cat_query', { port, settings, protocol, civAddress, timeoutMs }),
    pttTest: (port, line, durationMs) => invoke('diag_ptt_test', { port, line, durationMs }),
    getPortUsage: () => invoke('get_port_usage'),
    watchPort: (port, deviceId, thenOpen) => invoke('watch_port', { port, deviceId, thenOpen }),
    unwatchPort: (port) => invoke('unwatch_port', { port }),
    getWatchProgram: (deviceId) => invoke('get_watch_program', { deviceId }),
    pickProgram: () => invoke('pick_program'),
    onUsageUpdated: (cb) => listen<UsageView>('usage://updated', (e) => cb(e.payload)),
    onUsageReleased: (cb) => listen<ReleasedEvent>('usage://released', (e) => cb(e.payload)),
    onInventoryUpdated: (cb) => listen<InventoryView>('inventory://updated', (e) => cb(e.payload)),
    onInventoryEvents: (cb) => listen<InventoryEvent[]>('inventory://events', (e) => cb(e.payload)),
    onUpdateStatus: (cb) => listen<UpdateStatus>('updater://status', (e) => cb(e.payload)),
  };
}
