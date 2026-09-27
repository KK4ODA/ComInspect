import { errorMessage, getBackend, type Backend, type Unlisten } from './api';
import {
  applyFilters,
  type Filters,
  type PurposeFilter,
  type Sort,
  type SortColumn,
  type StatusFilter,
  type TypeFilter,
} from './filters';
import type {
  AppInfo,
  DeviceDetail,
  IdentityPatch,
  ImportMode,
  InventoryEvent,
  InventoryView,
  PortRow,
  PortUsageView,
  UiPrefs,
  UpdateStatus,
  UsageView,
} from './types';

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'warning' | 'error';
  title: string;
  detail?: string;
  action?: { label: string; run: () => void };
}

export type DialogKind = 'about' | 'import' | 'backups' | 'merge' | 'confirm' | null;

export interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
}

let toastSeq = 0;

class AppStore {
  backend: Backend | null = null;
  private unlisten: Unlisten[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;
  private prefsTimer: ReturnType<typeof setTimeout> | null = null;
  private detailRequest = 0;

  view = $state<InventoryView | null>(null);
  loading = $state(true);
  refreshing = $state(false);
  fatal = $state<string | null>(null);

  search = $state('');
  status = $state<StatusFilter>('all');
  types = $state<TypeFilter[]>([]);
  purposes = $state<PurposeFilter[]>([]);
  showIgnored = $state(false);
  sort = $state<Sort>({ column: 'status', dir: 1 });

  selectedId = $state<number | null>(null);
  detail = $state<DeviceDetail | null>(null);
  inspectorOpen = $state(true);
  focusNickname = $state(0);

  toasts = $state<Toast[]>([]);
  dialog = $state<DialogKind>(null);
  confirmRequest = $state<ConfirmRequest | null>(null);

  appInfo = $state<AppInfo | null>(null);
  update = $state<UpdateStatus | null>(null);
  usage = $state<UsageView | null>(null);
  collapsed = $state<Record<string, boolean>>({ system: true, recognition: true, diagnostics: true });
  dismissedBanners = $state<string[]>([]);
  now = $state(Date.now());

  filters: Filters = $derived({
    search: this.search,
    status: this.status,
    types: this.types,
    purposes: this.purposes,
    showIgnored: this.showIgnored,
  });
  rows: PortRow[] = $derived(this.view ? applyFilters(this.view.rows, this.filters, this.sort) : []);
  selectedRow: PortRow | null = $derived(
    this.view?.rows.find((r) => r.deviceId === this.selectedId) ?? null,
  );
  equipmentSuggestions: string[] = $derived(
    [...new Set((this.view?.rows ?? []).map((r) => r.equipment).filter((e): e is string => !!e))].sort(),
  );
  filtersActive: boolean = $derived(
    this.search.trim() !== '' || this.status !== 'all' || this.types.length > 0 || this.purposes.length > 0,
  );
  updateBadge: boolean = $derived(
    this.update?.phase === 'available' || this.update?.phase === 'ready_to_restart',
  );

  async init(): Promise<void> {
    try {
      const b = await getBackend();
      this.backend = b;
      const [view, prefs, info] = await Promise.all([
        b.getInventory(),
        b.getUiPrefs().catch(() => null),
        b.getAppInfo().catch(() => null),
      ]);
      this.applyPrefs(prefs);
      this.appInfo = info;
      this.update = info?.updates ?? null;
      this.applyView(view);
      const firstConnected = this.rows.find((r) => r.status === 'connected' || r.status === 'problem');
      if (firstConnected) this.select(firstConnected.deviceId);
      this.loading = false;

      this.unlisten.push(
        await b.onInventoryUpdated((v) => this.applyView(v)),
        await b.onInventoryEvents((events) => this.announce(events)),
        await b.onUpdateStatus((s) => (this.update = s)),
        await b.onUsageUpdated((u) => (this.usage = u)),
        await b.onUsageReleased((e) => {
          if (e.error) this.toast('warning', `${e.port} is free`, e.message);
          else this.toast('success', `${e.port} is free`, e.message);
        }),
      );
      this.usage = await b.getPortUsage().catch(() => null);
      this.timer = setInterval(() => (this.now = Date.now()), 30_000);

      if (info?.startup.updatedFrom) {
        this.toast('success', `Updated to ComInspect ${info.version}`, `Previously ${info.startup.updatedFrom}.`, {
          label: "What's new",
          run: () => void this.backend?.openLink('releases', info.version),
        });
      }
      // Launch confirmation for the update recovery logic.
      requestAnimationFrame(() => void b.appReady().catch(() => undefined));
    } catch (e) {
      this.fatal = errorMessage(e);
      this.loading = false;
    }
  }

  dispose(): void {
    this.unlisten.forEach((u) => u());
    this.unlisten = [];
    if (this.timer) clearInterval(this.timer);
  }

  private applyPrefs(prefs: UiPrefs | null): void {
    if (!prefs) return;
    if (prefs.sort) this.sort = prefs.sort as Sort;
    if (typeof prefs.showIgnored === 'boolean') this.showIgnored = prefs.showIgnored;
    if (prefs.collapsed) this.collapsed = { ...this.collapsed, ...prefs.collapsed };
  }

  savePrefs(): void {
    if (this.prefsTimer) clearTimeout(this.prefsTimer);
    this.prefsTimer = setTimeout(() => {
      const prefs: UiPrefs = {
        sort: $state.snapshot(this.sort),
        showIgnored: this.showIgnored,
        collapsed: $state.snapshot(this.collapsed),
      };
      void this.backend?.setUiPrefs(prefs).catch(() => undefined);
    }, 500);
  }

  applyView(view: InventoryView): void {
    this.view = view;
    if (this.selectedId != null) {
      if (!view.rows.some((r) => r.deviceId === this.selectedId)) {
        this.selectedId = null;
        this.detail = null;
      } else {
        void this.loadDetail(this.selectedId);
      }
    }
  }

  // --- selection ------------------------------------------------------------------

  select(id: number | null): void {
    if (id === this.selectedId) return;
    this.selectedId = id;
    this.detail = null;
    if (id != null) {
      this.inspectorOpen = true;
      void this.loadDetail(id);
    }
  }

  async loadDetail(id: number): Promise<void> {
    if (!this.backend) return;
    const request = ++this.detailRequest;
    try {
      const detail = await this.backend.getDeviceDetail(id);
      if (request === this.detailRequest && this.selectedId === id) this.detail = detail;
    } catch (e) {
      if (request === this.detailRequest) this.error('Could not load device details', e);
    }
  }

  moveSelection(delta: number): void {
    if (!this.rows.length) return;
    const index = this.rows.findIndex((r) => r.deviceId === this.selectedId);
    const next = index < 0 ? 0 : Math.max(0, Math.min(this.rows.length - 1, index + delta));
    this.select(this.rows[next].deviceId);
  }

  // --- filters & sorting -------------------------------------------------------

  toggleType(t: TypeFilter): void {
    this.types = this.types.includes(t) ? this.types.filter((x) => x !== t) : [...this.types, t];
  }

  togglePurpose(p: PurposeFilter): void {
    this.purposes = this.purposes.includes(p) ? this.purposes.filter((x) => x !== p) : [...this.purposes, p];
  }

  clearFilters(): void {
    this.search = '';
    this.status = 'all';
    this.types = [];
    this.purposes = [];
  }

  setSort(column: SortColumn): void {
    this.sort = this.sort.column === column ? { column, dir: this.sort.dir === 1 ? -1 : 1 } : { column, dir: 1 };
    this.savePrefs();
  }

  setShowIgnored(value: boolean): void {
    this.showIgnored = value;
    this.savePrefs();
  }

  toggleSection(key: string): void {
    this.collapsed = { ...this.collapsed, [key]: !this.collapsed[key] };
    this.savePrefs();
  }

  // --- actions -----------------------------------------------------------------

  async refresh(): Promise<void> {
    if (!this.backend || this.refreshing) return;
    this.refreshing = true;
    try {
      this.applyView(await this.backend.refresh());
    } catch (e) {
      this.error('Refresh failed', e);
    } finally {
      this.refreshing = false;
    }
  }

  async updateIdentity(id: number, patch: IdentityPatch): Promise<boolean> {
    if (!this.backend) return false;
    try {
      this.applyView(await this.backend.updateIdentity(id, patch));
      return true;
    } catch (e) {
      this.error('Could not save', e);
      return false;
    }
  }

  async setIgnored(id: number, ignored: boolean): Promise<void> {
    if (!this.backend) return;
    try {
      this.applyView(await this.backend.setIgnored(id, ignored));
      this.toast(
        'info',
        ignored ? 'Port hidden from the list' : 'Port shown in the list',
        ignored ? 'Use the menu → "Show ignored ports" to see it again.' : undefined,
      );
    } catch (e) {
      this.error('Could not change the port', e);
    }
  }

  async merge(targetId: number, sourceId: number): Promise<void> {
    if (!this.backend) return;
    try {
      this.applyView(await this.backend.mergeDevices(targetId, sourceId));
      this.toast('success', 'Entries linked', 'Both USB sockets are now recognized as this device.');
    } catch (e) {
      this.error('Could not link the entries', e);
    }
  }

  async forget(id: number): Promise<void> {
    if (!this.backend) return;
    try {
      this.applyView(await this.backend.forgetDevice(id));
      this.toast('info', 'Device forgotten');
    } catch (e) {
      this.error('Could not forget the device', e);
    }
  }

  async exportInventory(): Promise<void> {
    if (!this.backend) return;
    try {
      const path = await this.backend.exportInventory();
      if (path) this.toast('success', 'Port mappings exported', path);
    } catch (e) {
      this.error('Export failed', e);
    }
  }

  async importInventory(mode: ImportMode): Promise<boolean> {
    if (!this.backend) return false;
    try {
      const report = await this.backend.importInventory(mode);
      if (!report) return false;
      const from = report.sourceHostname ? ` from ${report.sourceHostname}` : '';
      this.toast(
        'success',
        `Imported${from}`,
        `${report.matched} updated, ${report.created} waiting for their devices, ${report.unchanged} unchanged.`,
      );
      return true;
    } catch (e) {
      this.error('Import failed', e);
      return false;
    }
  }

  confirm(request: ConfirmRequest): void {
    this.confirmRequest = request;
    this.dialog = 'confirm';
  }

  // --- updates -----------------------------------------------------------------

  async checkForUpdates(): Promise<void> {
    if (!this.backend) return;
    try {
      this.update = await this.backend.checkForUpdates();
    } catch (e) {
      this.error('Update check failed', e);
    }
  }

  async installUpdate(): Promise<void> {
    if (!this.backend) return;
    try {
      await this.backend.installUpdate((e) => {
        if (e.event === 'progress' && this.update) {
          this.update = { ...this.update, phase: 'downloading', downloaded: e.data.downloaded, total: e.data.total };
        }
      });
    } catch (e) {
      this.error('The update was not installed', e);
    }
  }

  // --- port usage -----------------------------------------------------------------

  /** Which programs have the row's port open (connected ports only). */
  usageFor(row: PortRow | null | undefined): PortUsageView | null {
    if (!row?.port || !(row.status === 'connected' || row.status === 'problem')) return null;
    return this.usage?.ports[row.port] ?? null;
  }

  async watchPort(row: PortRow, thenOpen: string | null): Promise<void> {
    if (!this.backend || !row.port) return;
    try {
      this.usage = await this.backend.watchPort(row.port, row.deviceId, thenOpen);
    } catch (e) {
      this.error(`Could not watch ${row.port}`, e);
    }
  }

  async unwatchPort(port: string): Promise<void> {
    if (!this.backend) return;
    try {
      this.usage = await this.backend.unwatchPort(port);
    } catch (e) {
      this.error(`Could not stop watching ${port}`, e);
    }
  }

  /** The program last chosen to start when this device's port is free. */
  async watchProgram(deviceId: number): Promise<string | null> {
    return (await this.backend?.getWatchProgram(deviceId).catch(() => null)) ?? null;
  }

  async pickProgram(): Promise<string | null> {
    if (!this.backend) return null;
    try {
      return await this.backend.pickProgram();
    } catch (e) {
      this.error('Could not choose a program', e);
      return null;
    }
  }

  // --- notifications ------------------------------------------------------------

  toast(kind: Toast['kind'], title: string, detail?: string, action?: Toast['action']): void {
    const t: Toast = { id: ++toastSeq, kind, title, detail, action };
    this.toasts = [...this.toasts.slice(-3), t];
    setTimeout(() => this.dismissToast(t.id), action ? 9000 : 6000);
  }

  dismissToast(id: number): void {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  error(title: string, e: unknown): void {
    const message = errorMessage(e);
    this.toast('error', title, message);
    void this.backend?.log('error', `${title}: ${message}`).catch(() => undefined);
  }

  private announce(events: InventoryEvent[]): void {
    const live = events.filter((e) => !e.initial);
    if (live.length > 3) {
      this.toast('info', `${live.length} serial port changes`, 'The list has been updated.');
      return;
    }
    for (const e of live) {
      const port = e.port ?? '';
      switch (e.kind) {
        case 'connected':
          this.toast(
            'success',
            `${e.label} connected`,
            e.previousPort ? `Now ${port} (was ${e.previousPort})` : port,
            { label: 'Show', run: () => this.reveal(e.deviceId) },
          );
          break;
        case 'disconnected':
          this.toast('info', `${e.label} disconnected`, port);
          break;
        case 'port_changed':
          this.toast('warning', `${e.label} moved to ${port}`, e.previousPort ? `Previously ${e.previousPort}` : undefined, {
            label: 'Show',
            run: () => this.reveal(e.deviceId),
          });
          break;
        case 'new_device':
          this.toast('info', `New serial device on ${port}`, e.label, {
            label: 'Name it',
            run: () => {
              this.reveal(e.deviceId);
              this.focusNickname++;
            },
          });
          break;
      }
    }
  }

  reveal(id: number): void {
    const visible = this.rows.some((r) => r.deviceId === id);
    if (!visible) this.clearFilters();
    this.select(id);
    requestAnimationFrame(() =>
      document.querySelector(`[data-device="${id}"]`)?.scrollIntoView({ block: 'nearest' }),
    );
  }
}

export const app = new AppStore();
