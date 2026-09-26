// TypeScript mirror of the Rust view model (serde camelCase).
// Rust sources: crates/cominspect-core/src/model.rs,
// crates/cominspect-store/src/view.rs, src-tauri/src/*.rs.

export type Transport = 'usb' | 'bluetooth' | 'virtual' | 'pci' | 'builtin' | 'unknown';
export type PortStatus = 'connected' | 'problem' | 'absent_os' | 'absent' | 'awaiting';
export type Severity = 'info' | 'warning' | 'error';
export type Purpose =
  | 'cat'
  | 'secondary_cat'
  | 'ptt'
  | 'cw'
  | 'programming'
  | 'data'
  | 'kiss'
  | 'gps'
  | 'control'
  | 'other'
  | 'unknown';
export type Category =
  | 'radio_cat'
  | 'ptt'
  | 'cw_keying'
  | 'tnc'
  | 'kiss'
  | 'gps'
  | 'rotator'
  | 'amplifier'
  | 'antenna_tuner'
  | 'antenna_switch'
  | 'programming_cable'
  | 'radio_interface'
  | 'bluetooth_serial'
  | 'virtual_serial'
  | 'generic_serial'
  | 'other';
export type CatStatus = 'verified' | 'not_cat' | 'unknown';

export interface PortRow {
  deviceId: number;
  uuid: string;
  status: PortStatus;
  port: string | null;
  portShort: string | null;
  previousPort: string | null;
  portChangedAt: number | null;
  nickname: string | null;
  equipment: string | null;
  category: Category | null;
  purpose: Purpose | null;
  catStatus: CatStatus;
  notes: string | null;
  ignored: boolean;
  deviceLabel: string;
  hintLabel: string | null;
  manufacturer: string | null;
  product: string | null;
  transport: Transport;
  vid: number | null;
  pid: number | null;
  serialNumber: string | null;
  interfaceNumber: number | null;
  btAddress: string | null;
  virtualProvider: string | null;
  firstSeen: number;
  lastSeen: number | null;
  lastSeenSource: 'app' | 'os' | null;
  severity: Severity | null;
  findingCount: number;
}

export interface Finding {
  code: string;
  severity: Severity;
  title: string;
  detail: string;
  portName: string | null;
  ports: number[];
  deviceIds: number[];
}

export interface Summary {
  total: number;
  connected: number;
  disconnected: number;
  problems: number;
  warnings: number;
  ignored: number;
  awaiting: number;
}

export interface PlatformCapabilities {
  remembersAbsentDevices: boolean;
  reservedNumbers: boolean;
  osTimestamps: boolean;
  monitoring: string;
}

export interface ReservedPorts {
  numbers: number[];
  databaseSize: number;
  source: string;
}

export interface ScanMeta {
  platform: string;
  scannedAt: number;
  durationMs: number;
  warnings: string[];
  capabilities: PlatformCapabilities;
  reserved: ReservedPorts | null;
}

export interface StorageInfo {
  temporary: boolean;
  reason: string | null;
}

export interface InventoryView {
  rows: PortRow[];
  findings: Finding[];
  summary: Summary;
  scan: ScanMeta | null;
  storage: StorageInfo;
}

export interface UsbInfo {
  vid: number;
  pid: number;
  serialNumber: string | null;
  interfaceNumber: number | null;
  interfaceName: string | null;
  manufacturer: string | null;
  product: string | null;
  revision: number | null;
  location: string | null;
  locationLabel: string | null;
  deviceNode: string | null;
}

export interface BluetoothInfo {
  address: string | null;
  deviceName: string | null;
  direction: 'incoming' | 'outgoing' | null;
  service: string | null;
  serviceKey: string | null;
  channel: number | null;
}

export interface VirtualInfo {
  provider: string;
  detail: string | null;
  heuristic: boolean;
}

export interface SystemInfo {
  instanceId: string | null;
  hardwareIds: string[];
  compatibleIds: string[];
  parentInstanceId: string | null;
  containerId: string | null;
  deviceClass: string | null;
  classGuid: string | null;
  enumerator: string | null;
  driver: string | null;
  driverProvider: string | null;
  driverVersion: string | null;
  driverDate: string | null;
  driverInf: string | null;
  kernelName: string | null;
  devicePath: string | null;
  locationPaths: string[];
  locationInfo: string | null;
  problem: { code: number; description: string } | null;
  access: { readable: boolean; writable: boolean; ownerGroup: string | null; mode: string | null } | null;
}

export interface OsTimestamps {
  firstInstall: number | null;
  install: number | null;
  lastArrival: number | null;
  lastRemoval: number | null;
}

export interface DiscoveredPort {
  portName: string;
  aliases: string[];
  presence: 'present' | 'absent';
  transport: Transport;
  friendlyName: string | null;
  description: string | null;
  manufacturer: string | null;
  usb: UsbInfo | null;
  bluetooth: BluetoothInfo | null;
  virtualPort: VirtualInfo | null;
  system: SystemInfo;
  osTimes: OsTimestamps;
  instanceIdentity: { value: string; quality: 'device_unique' | 'stable' | 'location' } | null;
  notes: string[];
  extra: { name: string; value: string }[];
}

export interface Hint {
  id: string;
  title: string;
  detail: string | null;
  shortLabel: string | null;
  chip: string | null;
  suggestedPurpose: Purpose | null;
  suggestedCategory: Category | null;
  suggestedEquipment: string | null;
  caution: boolean;
  source: string;
}

export interface KeyRecord {
  kind: string;
  value: string;
  strength: number;
  portable: boolean;
  description: string;
  firstSeen: number;
  lastSeen: number;
}

export interface PortHistoryEntry {
  port: string;
  firstObserved: number;
  lastObserved: number;
  lastConnected: number | null;
  current: boolean;
}

export interface EventRecord {
  id: number;
  ts: number;
  deviceId: number | null;
  kind: string;
  port: string | null;
  previousPort: string | null;
  detail: string | null;
}

export interface MergeCandidate {
  deviceId: number;
  label: string;
  port: string | null;
  status: PortStatus;
  lastSeen: number | null;
  reason: string;
}

export interface ImportedFrom {
  hostname: string | null;
  os: string;
  lastPort: string | null;
  portHistory: string[];
  importedAt: number;
}

export type MatchBasis = 'usb-serial' | 'bluetooth' | 'os-device' | 'usb-path' | 'virtual' | 'port-name';

export interface DeviceDetail {
  row: PortRow;
  snapshot: DiscoveredPort | null;
  snapshotLive: boolean;
  recognizedBy: MatchBasis | null;
  identityNote: string;
  keys: KeyRecord[];
  portHistory: PortHistoryEntry[];
  events: EventRecord[];
  hints: Hint[];
  findings: Finding[];
  mergeCandidates: MergeCandidate[];
  importedFrom: ImportedFrom | null;
}

export type InventoryEventKind = 'new_device' | 'connected' | 'disconnected' | 'port_changed';

export interface InventoryEvent {
  kind: InventoryEventKind;
  deviceId: number;
  label: string;
  port: string | null;
  previousPort: string | null;
  ts: number;
  initial: boolean;
}

export interface IdentityPatch {
  nickname?: string | null;
  equipment?: string | null;
  category?: Category | null;
  purpose?: Purpose | null;
  catStatus?: CatStatus;
  notes?: string | null;
}

export type ImportMode = 'merge' | 'overwrite';

export interface ImportReport {
  matched: number;
  created: number;
  unchanged: number;
  sameMachine: boolean;
  sourceHostname: string | null;
  sourceOs: string;
}

export type UpdatePhase =
  | 'idle'
  | 'checking'
  | 'up_to_date'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'ready_to_restart'
  | 'error';

export type UpdateChannel = 'stable' | 'beta';

export interface UpdateStatus {
  currentVersion: string;
  configured: boolean;
  supported: boolean;
  phase: UpdatePhase;
  available: { version: string; date: string | null; notes: string | null } | null;
  lastChecked: number | null;
  error: string | null;
  channel: UpdateChannel;
  autoCheck: boolean;
  downloaded: number;
  total: number | null;
}

export type DownloadEvent =
  | { event: 'progress'; data: { downloaded: number; total: number | null } }
  | { event: 'installing' }
  | { event: 'finished' };

export interface StartupInfo {
  version: string;
  updatedFrom: string | null;
  firstRun: boolean;
  recovery: { failedVersion: string; lastGoodVersion: string | null; attempts: number } | null;
  previousVersion: string | null;
}

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  identifier: string;
  dataDir: string;
  dbPath: string;
  logDir: string;
  backupsDir: string | null;
  schemaVersion: number;
  deviceCount: number;
  keyCount: number;
  eventCount: number;
  storage: StorageInfo;
  databaseBackup: string | null;
  recoveredFile: string | null;
  migrationsApplied: number[];
  startup: StartupInfo;
  repository: string;
  updates: UpdateStatus;
}

export interface BackupInfo {
  path: string;
  fileName: string;
  size: number;
  modified: number | null;
  schema: number | null;
}

export interface SerialSettings {
  baudRate: number;
  dataBits: number;
  parity: 'none' | 'even' | 'odd';
  stopBits: number;
  flowControl: 'none' | 'hardware' | 'software';
}

export type CatProtocol =
  | 'kenwood_id'
  | 'kenwood_frequency'
  | 'icom_id'
  | 'icom_frequency'
  | 'yaesu_legacy_frequency';

export interface CatProbeInfo {
  protocol: CatProtocol;
  label: string;
  description: string;
  defaultBaud: number;
  defaultStopBits: number;
}

export interface ProbeCatalog {
  probes: CatProbeInfo[];
  icomAddresses: [number, string][];
  maxPttMs: number;
}

export type OpenOutcome = 'opened' | 'in_use' | 'permission_denied' | 'not_found' | 'error';

export interface OpenTestReport {
  port: string;
  outcome: OpenOutcome;
  message: string;
  lines: { cts: boolean | null; dsr: boolean | null; dcd: boolean | null; ri: boolean | null } | null;
  users: { pid: number; name: string }[];
  elapsedMs: number;
}

export interface CatQueryReport {
  port: string;
  outcome: OpenOutcome;
  baudRate: number;
  triedRates: number[];
  sentHex: string;
  receivedHex: string;
  receivedText: string;
  recognized: boolean;
  summary: string;
  elapsedMs: number;
}

export type ControlLine = 'rts' | 'dtr';

export interface PttTestReport {
  port: string;
  outcome: OpenOutcome;
  line: ControlLine;
  heldMs: number;
  message: string;
}

export interface UiPrefs {
  sort?: { column: string; dir: 1 | -1 };
  showIgnored?: boolean;
  inspectorWidth?: number;
  collapsed?: Record<string, boolean>;
}
