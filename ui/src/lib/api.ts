import type {
  AppInfo,
  BackupInfo,
  CatProtocol,
  CatQueryReport,
  ControlLine,
  DeviceDetail,
  DownloadEvent,
  IdentityPatch,
  ImportMode,
  ImportReport,
  InventoryEvent,
  InventoryView,
  OpenTestReport,
  ProbeCatalog,
  PttTestReport,
  ReleasedEvent,
  SerialSettings,
  UiPrefs,
  UpdateChannel,
  UpdateStatus,
  UsageView,
} from './types';

export type Unlisten = () => void;

/** Everything the UI can ask of the application. */
export interface Backend {
  readonly kind: 'tauri' | 'mock';
  getInventory(): Promise<InventoryView>;
  refresh(): Promise<InventoryView>;
  getDeviceDetail(deviceId: number): Promise<DeviceDetail>;
  updateIdentity(deviceId: number, patch: IdentityPatch): Promise<InventoryView>;
  setIgnored(deviceId: number, ignored: boolean): Promise<InventoryView>;
  mergeDevices(targetId: number, sourceId: number): Promise<InventoryView>;
  forgetDevice(deviceId: number): Promise<InventoryView>;
  exportInventory(): Promise<string | null>;
  importInventory(mode: ImportMode): Promise<ImportReport | null>;
  getAppInfo(): Promise<AppInfo>;
  appReady(): Promise<void>;
  getUiPrefs(): Promise<UiPrefs | null>;
  setUiPrefs(prefs: UiPrefs): Promise<void>;
  log(level: 'info' | 'warn' | 'error', message: string): Promise<void>;
  openLocation(which: 'data' | 'logs' | 'backups'): Promise<void>;
  openLink(which: 'homepage' | 'releases' | 'issues', version?: string): Promise<void>;
  getUpdateStatus(): Promise<UpdateStatus>;
  checkForUpdates(): Promise<UpdateStatus>;
  installUpdate(onEvent: (e: DownloadEvent) => void): Promise<void>;
  reinstallVersion(version: string, onEvent: (e: DownloadEvent) => void): Promise<void>;
  setAutoUpdateCheck(enabled: boolean): Promise<UpdateStatus>;
  setUpdateChannel(channel: UpdateChannel): Promise<UpdateStatus>;
  restartApp(): Promise<void>;
  listBackups(): Promise<BackupInfo[]>;
  createBackup(): Promise<string>;
  restoreBackup(fileName: string): Promise<InventoryView>;
  probeCatalog(): Promise<ProbeCatalog>;
  openTest(port: string): Promise<OpenTestReport>;
  catQuery(
    port: string,
    settings: SerialSettings,
    protocol: CatProtocol,
    civAddress: number,
    timeoutMs: number,
  ): Promise<CatQueryReport>;
  pttTest(port: string, line: ControlLine, durationMs: number): Promise<PttTestReport>;
  getPortUsage(): Promise<UsageView>;
  /** Waits for `port` to be released; optionally starts `thenOpen` then. */
  watchPort(port: string, deviceId: number | null, thenOpen: string | null): Promise<UsageView>;
  unwatchPort(port: string): Promise<UsageView>;
  /** The program last chosen to start when this device's port is free. */
  getWatchProgram(deviceId: number): Promise<string | null>;
  pickProgram(): Promise<string | null>;
  onUsageUpdated(cb: (view: UsageView) => void): Promise<Unlisten>;
  onUsageReleased(cb: (event: ReleasedEvent) => void): Promise<Unlisten>;
  onInventoryUpdated(cb: (view: InventoryView) => void): Promise<Unlisten>;
  onInventoryEvents(cb: (events: InventoryEvent[]) => void): Promise<Unlisten>;
  onUpdateStatus(cb: (status: UpdateStatus) => void): Promise<Unlisten>;
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

let backend: Promise<Backend> | null = null;

/** The Tauri backend inside the app; an in-memory mock in a plain browser
 * (development, screenshots and UI tests). */
export function getBackend(): Promise<Backend> {
  if (!backend) {
    backend = isTauri()
      ? import('./tauri-backend').then((m) => m.createTauriBackend())
      : import('./mock').then((m) => m.createMockBackend());
  }
  return backend;
}

/** Turns a rejected command into a readable message. */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
