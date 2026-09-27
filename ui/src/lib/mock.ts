// In-memory backend used when the UI runs in a plain browser (development,
// screenshots and UI tests). It models a typical Windows ham-radio station.
import type { Backend, Unlisten } from './api';
import type {
  AppInfo,
  DeviceDetail,
  DiscoveredPort,
  DownloadEvent,
  Finding,
  Hint,
  InventoryEvent,
  InventoryView,
  PortHolder,
  PortRow,
  ReleasedEvent,
  Transport,
  UpdateStatus,
  UsageView,
  VspeView,
} from './types';

const DAY = 86_400_000;
/** The demo runs this version and always offers the next minor one. */
const APP_VERSION = __APP_VERSION__;
const NEXT_VERSION = APP_VERSION.replace(/^(\d+)\.(\d+)\..*$/, (_, major: string, minor: string) => `${major}.${Number(minor) + 1}.0`);
const now = Date.now();

interface MockDevice {
  row: PortRow;
  snapshot: DiscoveredPort;
  hints: Hint[];
  history: { port: string; first: number; last: number }[];
}

function row(partial: Partial<PortRow> & Pick<PortRow, 'deviceId' | 'deviceLabel' | 'transport' | 'status'>): PortRow {
  return {
    uuid: `mock-${partial.deviceId}`,
    port: null,
    previousPort: null,
    portChangedAt: null,
    nickname: null,
    equipment: null,
    category: null,
    purpose: null,
    catStatus: 'unknown',
    notes: null,
    ignored: false,
    hintLabel: null,
    manufacturer: null,
    product: null,
    vid: null,
    pid: null,
    serialNumber: null,
    interfaceNumber: null,
    btAddress: null,
    virtualProvider: null,
    firstSeen: now - 120 * DAY,
    lastSeen: now,
    lastSeenSource: 'app',
    severity: null,
    findingCount: 0,
    ...partial,
    portShort: partial.port ?? null,
  };
}

function snapshot(
  r: PortRow,
  extra: Partial<DiscoveredPort> & { instanceId: string; driver: string; provider?: string; version?: string },
): DiscoveredPort {
  const present = r.status === 'connected' || r.status === 'problem';
  return {
    portName: r.port ?? '',
    aliases: [],
    presence: present ? 'present' : 'absent',
    transport: r.transport,
    friendlyName: `${extra.description ?? r.deviceLabel} (${r.port})`,
    description: extra.description ?? r.deviceLabel,
    manufacturer: r.manufacturer,
    usb:
      r.vid != null && r.pid != null
        ? {
            vid: r.vid,
            pid: r.pid,
            serialNumber: r.serialNumber,
            interfaceNumber: r.interfaceNumber ?? 0,
            interfaceName: null,
            manufacturer: null,
            product: r.product,
            revision: 0x0100,
            location: 'PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)#USB(3)',
            locationLabel: 'Port_#0003.Hub_#0002',
            deviceNode: `USB\\VID_${r.vid.toString(16).toUpperCase().padStart(4, '0')}&PID_${r.pid
              .toString(16)
              .toUpperCase()
              .padStart(4, '0')}\\${r.serialNumber ?? '5&2A3B4C5D&0&3'}`,
          }
        : null,
    bluetooth: null,
    virtualPort: null,
    system: {
      instanceId: extra.instanceId,
      hardwareIds: [extra.instanceId.split('\\').slice(0, 2).join('\\')],
      compatibleIds: [],
      parentInstanceId: null,
      containerId: '{8a1c4b52-0d8e-5f3a-9c1e-2b7d4e6f8a90}',
      deviceClass: 'Ports',
      classGuid: '{4d36e978-e325-11ce-bfc1-08002be10318}',
      enumerator: extra.instanceId.split('\\')[0],
      driver: extra.driver,
      driverProvider: extra.provider ?? null,
      driverVersion: extra.version ?? null,
      driverDate: '2023-05-18',
      driverInf: 'oem42.inf',
      kernelName: present ? `\\Device\\${extra.driver}0` : null,
      devicePath: null,
      locationPaths: r.vid != null ? ['PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)#USB(3)'] : [],
      locationInfo: r.vid != null ? 'Port_#0003.Hub_#0002' : null,
      problem: null,
      access: null,
    },
    osTimes: {
      firstInstall: now - 400 * DAY,
      install: now - 400 * DAY,
      lastArrival: present ? now - 2 * 3600_000 : (r.lastSeen ?? now) - 3600_000,
      lastRemoval: present ? null : r.lastSeen,
    },
    instanceIdentity: { value: extra.instanceId, quality: r.serialNumber ? 'device_unique' : 'location' },
    notes: [],
    extra: [],
    ...extra,
  } as DiscoveredPort;
}

function hint(id: string, title: string, detail: string, more: Partial<Hint> = {}): Hint {
  return {
    id,
    title,
    detail,
    shortLabel: null,
    chip: null,
    suggestedPurpose: null,
    suggestedCategory: null,
    suggestedEquipment: null,
    caution: false,
    source: 'ComInspect built-in device hints',
    ...more,
  };
}

function buildDevices(): MockDevice[] {
  const devices: MockDevice[] = [];
  const add = (r: PortRow, s: DiscoveredPort, hints: Hint[] = [], history?: MockDevice['history']) =>
    devices.push({
      row: r,
      snapshot: s,
      hints,
      history: history ?? [{ port: r.port ?? '', first: r.firstSeen, last: r.lastSeen ?? now }],
    });

  const enhanced = row({
    deviceId: 1,
    status: 'connected',
    port: 'COM7',
    previousPort: 'COM5',
    portChangedAt: now - 2 * DAY,
    nickname: 'FTDX10 CAT Enhanced',
    equipment: 'Yaesu FTDX10',
    category: 'radio_cat',
    purpose: 'cat',
    catStatus: 'verified',
    notes: 'Enhanced port — use this one for OmniRig, WSJT-X and N1MM (38400 baud).',
    deviceLabel: 'Silicon Labs Dual CP2105 USB to UART Bridge: Enhanced COM Port',
    hintLabel: 'Enhanced COM port',
    manufacturer: 'Silicon Labs',
    product: 'CP2105 Dual USB to UART Bridge Controller',
    transport: 'usb',
    vid: 0x10c4,
    pid: 0xea70,
    serialNumber: '01A2B3C4',
    interfaceNumber: 0,
    firstSeen: now - 210 * DAY,
  });
  add(
    enhanced,
    snapshot(enhanced, {
      instanceId: 'USB\\VID_10C4&PID_EA70&MI_00\\6&2A3B4C5D&0&0000',
      driver: 'silabser',
      provider: 'Silicon Laboratories Inc.',
      version: '10.1.10.0',
    }),
    [
      hint(
        'silabs-cp2105-enhanced',
        'CP2105 Enhanced COM port (interface 0)',
        "Interface 0 of a CP2105 is its Enhanced COM port. Yaesu radios with a built-in CP2105 (for example FT-991/FT-991A, FTDX10 and the FTDX101 series) document the Enhanced port for CAT control. Verify against your radio's manual, then mark the port as Verified CAT.",
        { shortLabel: 'Enhanced COM port', suggestedPurpose: 'cat', chip: 'Silicon Labs CP2105 dual UART' },
      ),
    ],
    [
      { port: 'COM7', first: now - 2 * DAY, last: now },
      { port: 'COM5', first: now - 210 * DAY, last: now - 2 * DAY - 3600_000 },
    ],
  );

  const standard = row({
    deviceId: 2,
    status: 'connected',
    port: 'COM8',
    nickname: 'FTDX10 PTT / CW',
    equipment: 'Yaesu FTDX10',
    category: 'ptt',
    purpose: 'ptt',
    catStatus: 'not_cat',
    notes: 'RTS = PTT, DTR = CW keying.',
    deviceLabel: 'Silicon Labs Dual CP2105 USB to UART Bridge: Standard COM Port',
    hintLabel: 'Standard COM port',
    manufacturer: 'Silicon Labs',
    product: 'CP2105 Dual USB to UART Bridge Controller',
    transport: 'usb',
    vid: 0x10c4,
    pid: 0xea70,
    serialNumber: '01A2B3C4',
    interfaceNumber: 1,
    firstSeen: now - 210 * DAY,
  });
  add(
    standard,
    snapshot(standard, {
      instanceId: 'USB\\VID_10C4&PID_EA70&MI_01\\6&2A3B4C5D&0&0001',
      driver: 'silabser',
      provider: 'Silicon Laboratories Inc.',
      version: '10.1.10.0',
    }),
    [
      hint(
        'silabs-cp2105-standard',
        'CP2105 Standard COM port (interface 1)',
        'Interface 1 of a CP2105 is its Standard COM port. On Yaesu radios with a built-in CP2105 it is normally used for transmit control through RTS/DTR (PTT, CW and FSK keying) rather than for CAT commands.',
        { shortLabel: 'Standard COM port', suggestedPurpose: 'ptt' },
      ),
    ],
  );

  const icom = row({
    deviceId: 3,
    status: 'connected',
    port: 'COM4',
    nickname: 'IC-7300 CI-V',
    equipment: 'Icom IC-7300',
    category: 'radio_cat',
    purpose: 'cat',
    catStatus: 'verified',
    deviceLabel: 'Silicon Labs CP210x USB to UART Bridge',
    hintLabel: 'Icom IC-7300',
    manufacturer: 'Silicon Labs',
    product: 'CP2102 USB to UART Bridge Controller',
    transport: 'usb',
    vid: 0x10c4,
    pid: 0xea60,
    serialNumber: 'IC-7300 03001234',
    firstSeen: now - 365 * DAY,
  });
  add(
    icom,
    snapshot(icom, {
      instanceId: 'USB\\VID_10C4&PID_EA60\\IC-7300_03001234',
      driver: 'silabser',
      provider: 'Silicon Laboratories Inc.',
      version: '10.1.10.0',
    }),
    [
      hint(
        'icom-cp210x-serial',
        'Icom IC-7300',
        'The USB serial string identifies this port as belonging to an Icom IC-7300. Icom radios use a USB serial port for CI-V (CAT) control.',
        { shortLabel: 'Icom IC-7300', suggestedEquipment: 'Icom IC-7300', suggestedCategory: 'radio_cat' },
      ),
    ],
  );

  const vara = row({
    deviceId: 4,
    status: 'connected',
    port: 'COM12',
    nickname: 'VARA KISS',
    category: 'kiss',
    purpose: 'kiss',
    notes: 'VARA side of the com0com pair (COM12 ↔ COM13).',
    deviceLabel: 'com0com - serial port emulator',
    transport: 'virtual',
    virtualProvider: 'com0com',
    firstSeen: now - 90 * DAY,
  });
  const varaSnap = snapshot(vara, {
    instanceId: 'COM0COM\\PORT\\CNCA0',
    driver: 'com0com',
    provider: 'Vyacheslav Frolov',
    version: '3.0.0.0',
  });
  varaSnap.virtualPort = { provider: 'com0com', detail: 'Paired with COM13 (CNCB0)', heuristic: false };
  add(vara, varaSnap, [
    hint('com0com', 'com0com null-modem pair', 'Data written to one port of a com0com pair comes out of the other.', {
      suggestedCategory: 'virtual_serial',
    }),
  ]);

  const winlink = row({
    deviceId: 5,
    status: 'connected',
    port: 'COM13',
    nickname: 'Winlink KISS Port',
    category: 'kiss',
    purpose: 'kiss',
    deviceLabel: 'com0com - serial port emulator',
    transport: 'virtual',
    virtualProvider: 'com0com',
    firstSeen: now - 90 * DAY,
  });
  const winlinkSnap = snapshot(winlink, {
    instanceId: 'COM0COM\\PORT\\CNCB0',
    driver: 'com0com',
    provider: 'Vyacheslav Frolov',
    version: '3.0.0.0',
  });
  winlinkSnap.virtualPort = { provider: 'com0com', detail: 'Paired with COM12 (CNCA0)', heuristic: false };
  add(winlink, winlinkSnap);

  const gps = row({
    deviceId: 6,
    status: 'absent_os',
    port: 'COM16',
    nickname: 'GPS Receiver',
    category: 'gps',
    purpose: 'gps',
    deviceLabel: 'u-blox 7 - GPS/GNSS Receiver',
    manufacturer: 'u-blox AG',
    product: 'u-blox 7 - GPS/GNSS Receiver',
    transport: 'usb',
    vid: 0x1546,
    pid: 0x01a7,
    lastSeen: now - 23 * DAY,
    firstSeen: now - 300 * DAY,
  });
  add(
    gps,
    snapshot(gps, { instanceId: 'USB\\VID_1546&PID_01A7\\5&11AA22BB&0&4', driver: 'usbser', provider: 'Microsoft' }),
    [
      hint('ublox-7', 'u-blox 7 GNSS receiver', 'Streams NMEA position and time data, useful for station clock synchronization and grid-square location.', {
        suggestedPurpose: 'gps',
        suggestedCategory: 'gps',
      }),
    ],
  );

  const prolific = row({
    deviceId: 7,
    status: 'problem',
    port: 'COM11',
    nickname: 'TS-480 Programming Cable',
    equipment: 'Kenwood TS-480',
    category: 'programming_cable',
    purpose: 'programming',
    deviceLabel: 'Prolific USB-to-Serial Comm Port',
    manufacturer: 'Prolific',
    transport: 'usb',
    vid: 0x067b,
    pid: 0x2303,
    severity: 'error',
    findingCount: 2,
    firstSeen: now - 60 * DAY,
  });
  const prolificSnap = snapshot(prolific, {
    instanceId: 'USB\\VID_067B&PID_2303\\5&3C4D5E6F&0&1',
    driver: 'Ser2pl',
    provider: 'Prolific',
    version: '3.9.6.2',
  });
  prolificSnap.system.problem = { code: 10, description: 'This device cannot start. (Code 10)' };
  add(prolific, prolificSnap, [
    hint(
      'prolific-pl2303',
      'Prolific PL2303 USB-serial bridge',
      'Very common in inexpensive radio programming cables. Current Prolific drivers refuse counterfeit and discontinued PL2303 variants (Device Manager Code 10). These cables usually report no serial number, so they are recognized by the USB socket they are plugged into.',
      { caution: true },
    ),
  ]);

  const digirig = row({
    deviceId: 8,
    status: 'connected',
    port: 'COM9',
    nickname: 'Digirig PTT',
    equipment: 'Digirig Mobile',
    category: 'radio_interface',
    purpose: 'ptt',
    notes: 'RTS keys the handheld via the Digirig cable.',
    deviceLabel: 'Silicon Labs CP210x USB to UART Bridge',
    manufacturer: 'Silicon Labs',
    product: 'CP2102N USB to UART Bridge Controller',
    transport: 'usb',
    vid: 0x10c4,
    pid: 0xea60,
    serialNumber: '8c2a1f5e7bd3ec11',
    firstSeen: now - 45 * DAY,
  });
  add(digirig, snapshot(digirig, { instanceId: 'USB\\VID_10C4&PID_EA60\\8C2A1F5E7BD3EC11', driver: 'silabser', provider: 'Silicon Laboratories Inc.', version: '10.1.10.0' }));

  const bt = row({
    deviceId: 9,
    status: 'absent_os',
    port: 'COM20',
    nickname: 'Bluetooth TNC',
    category: 'tnc',
    purpose: 'kiss',
    deviceLabel: 'Standard Serial over Bluetooth link',
    manufacturer: 'Microsoft',
    transport: 'bluetooth',
    btAddress: '98:D3:31:F5:B4:C2',
    lastSeen: now - 5 * DAY,
    firstSeen: now - 150 * DAY,
  });
  const btSnap = snapshot(bt, {
    instanceId: 'BTHENUM\\{00001101-0000-1000-8000-00805F9B34FB}_LOCALMFG&0002\\7&2E1E2F58&0&98D331F5B4C2_C00000000',
    driver: 'BTHMODEM',
    provider: 'Microsoft',
  });
  btSnap.bluetooth = {
    address: '98:D3:31:F5:B4:C2',
    deviceName: 'TNC4',
    direction: 'outgoing',
    service: 'Serial Port (SPP)',
    serviceKey: null,
    channel: null,
  };
  add(bt, btSnap);

  const ch340 = row({
    deviceId: 10,
    status: 'absent',
    port: 'COM14',
    deviceLabel: 'USB-SERIAL CH340',
    manufacturer: 'wch.cn',
    transport: 'usb',
    vid: 0x1a86,
    pid: 0x7523,
    lastSeen: now - 64 * DAY,
    firstSeen: now - 70 * DAY,
  });
  add(ch340, snapshot(ch340, { instanceId: 'USB\\VID_1A86&PID_7523\\5&2A3B4C5D&0&3', driver: 'CH341SER_A64', provider: 'wch.cn' }), [
    hint('wch-ch340', 'WCH CH340 USB-serial bridge', 'The CH340 has no serial number, so identical cables can only be told apart by USB socket.'),
  ]);

  const amt = row({
    deviceId: 11,
    status: 'connected',
    port: 'COM3',
    deviceLabel: 'Intel(R) Active Management Technology - SOL',
    hintLabel: 'Intel AMT SOL (no connector)',
    manufacturer: 'Intel',
    transport: 'pci',
    firstSeen: now - 400 * DAY,
  });
  add(amt, snapshot(amt, { instanceId: 'PCI\\VEN_8086&DEV_A13D&SUBSYS_86941043&REV_31\\3&11583659&0&B3', driver: 'Serial', provider: 'Intel' }), [
    hint('intel-amt-sol', 'Intel AMT Serial-over-LAN port', "A remote-management port provided by the computer's Intel ME firmware. It has no physical connector and is not useful for radio equipment; you can safely ignore it.", {
      shortLabel: 'Intel AMT SOL (no connector)',
      suggestedCategory: 'other',
    }),
  ]);

  const com1 = row({
    deviceId: 12,
    status: 'connected',
    port: 'COM1',
    deviceLabel: 'Communications Port',
    manufacturer: '(Standard port types)',
    transport: 'builtin',
    firstSeen: now - 400 * DAY,
  });
  add(com1, snapshot(com1, { instanceId: 'ACPI\\PNP0501\\1', driver: 'Serial', provider: 'Microsoft' }), [
    hint('builtin-uart', 'Built-in serial port', 'A serial port on the motherboard. On PCs this is usually an RS-232 header or DB-9 connector.'),
  ]);

  // A VSPE splitter shares the FTDX10's CAT port (COM7) as COM21 and COM22.
  for (const [deviceId, port, nickname, purpose] of [
    [13, 'COM21', 'WSJT-X CAT (shared)', 'cat'],
    [14, 'COM22', 'N1MM CAT (shared)', 'secondary_cat'],
  ] as const) {
    const shared = row({
      deviceId,
      status: 'connected',
      port,
      nickname,
      equipment: 'Yaesu FTDX10',
      category: 'radio_cat',
      purpose,
      deviceLabel: 'Eterlogic Virtual Serial Port',
      manufacturer: 'Eterlogic',
      transport: 'virtual',
      virtualProvider: 'VSPE',
      firstSeen: now - 30 * DAY,
    });
    const snap = snapshot(shared, { instanceId: `ROOT\\PORTS\\000${deviceId - 12}`, driver: 'vspe', provider: 'Eterlogic' });
    snap.virtualPort = { provider: 'VSPE', detail: null, heuristic: true };
    add(shared, snap);
  }

  return devices;
}

const findingsFor = (devices: MockDevice[]): Finding[] => [
  {
    code: 'device-problem',
    severity: 'error',
    title: 'COM11 has a driver problem',
    detail:
      'This device cannot start. (Code 10) The Prolific driver refused to start this chip. This almost always means a counterfeit or discontinued PL2303 chip, which is very common in inexpensive radio programming cables. Options: install an older Prolific driver that still supports the chip, or replace the cable with an FTDI- or CP210x-based one.',
    portName: 'COM11',
    ports: [],
    deviceIds: [7],
  },
  {
    code: 'high-port-number',
    severity: 'info',
    title: 'COM11 is a high port number',
    detail:
      'Some older programs (including some radio programming software) can only open COM1–COM9, or list only COM1–COM16. 5 lower numbers are reserved by disconnected devices or stale reservations, which is why this port received a high number.',
    portName: 'COM11',
    ports: [],
    deviceIds: [7],
  },
  {
    code: 'stale-reservation',
    severity: 'info',
    title: '3 COM numbers are reserved without a device',
    detail:
      'COM2, COM5, COM6 are marked as in use in the Windows COM port database, but no device (connected or hidden) claims them. These reservations are left behind by uninstalled devices and push new devices to higher numbers.',
    portName: null,
    ports: [],
    deviceIds: [],
  },
  {
    code: 'absent-reservations',
    severity: 'info',
    title: `${devices.filter((d) => d.row.status === 'absent_os').length} COM numbers are held by disconnected devices`,
    detail:
      'Windows keeps a COM number reserved for every serial device it has ever seen, even when the device is unplugged. Removing hidden devices you no longer use (Device Manager → View → Show hidden devices) frees their numbers.',
    portName: null,
    ports: [],
    deviceIds: [],
  },
];

function transportOf(t: Transport): Transport {
  return t;
}

export function createMockBackend(): Backend {
  let devices = buildDevices();
  const listeners = {
    inventory: new Set<(v: InventoryView) => void>(),
    events: new Set<(e: InventoryEvent[]) => void>(),
    update: new Set<(s: UpdateStatus) => void>(),
  };
  let update: UpdateStatus = {
    currentVersion: APP_VERSION,
    configured: true,
    supported: true,
    phase: 'idle',
    available: null,
    lastChecked: now - 3 * 3600_000,
    error: null,
    channel: 'stable',
    autoCheck: true,
    downloaded: 0,
    total: null,
  };
  let prefs = {};
  const delay = <T>(value: T, ms = 60): Promise<T> => new Promise((r) => setTimeout(() => r(value), ms));

  // Programs holding ports in the demo station.
  const usageListeners = new Set<(v: UsageView) => void>();
  const releasedListeners = new Set<(e: ReleasedEvent) => void>();
  const demoHolders: Record<string, PortHolder> = {
    COM4: {
      pid: 6120,
      processName: 'wsjtx.exe',
      executable: 'C:\\WSJT\\wsjtx\\bin\\wsjtx.exe',
      description: 'WSJT-X',
    },
    COM9: {
      pid: 4312,
      processName: 'VARAFM.exe',
      executable: 'C:\\VARA FM\\VARAFM.exe',
      description: 'VARA FM',
    },
    COM7: {
      pid: 2204,
      processName: 'EterlogicVspeDeviceManagerService.exe',
      executable: 'C:\\Program Files\\Eterlogic\\VSPE\\EterlogicVspeDeviceManagerService.exe',
      description: 'VSPE service',
    },
    COM21: {
      pid: 7340,
      processName: 'wsjtx.exe',
      executable: 'C:\\WSJT\\wsjtx\\bin\\wsjtx.exe',
      description: 'WSJT-X',
    },
    COM22: {
      pid: 5528,
      processName: 'N1MMLogger.net.exe',
      executable: 'C:\\Program Files (x86)\\N1MM Logger+\\N1MMLogger.net.exe',
      description: 'N1MM Logger+',
    },
  };
  const usage: UsageView = { ports: {}, limitation: null, checkedAt: Date.now() };
  for (const d of devices) {
    const port = d.row.port;
    if (!port || !(d.row.status === 'connected' || d.row.status === 'problem')) continue;
    const holder = demoHolders[port];
    usage.ports[port] = holder
      ? { state: 'in_use', holders: [holder], since: now - 42 * 60_000, sinceExact: true, watch: null }
      : { state: 'free', since: now - 3 * 3600_000, sinceExact: false, watch: null };
  }
  const programs = new Map<number, string>([[8, 'C:\\VarAC\\VarAC.exe']]);
  const vspe: VspeView = {
    file: 'C:\\ProgramData\\Eterlogic\\VSPE\\autostart.vspe',
    chosen: false,
    devices: [
      {
        kind: 'Splitter',
        settings: 'v2;7;28;v003,38400,8,0,0,0,2,2,0,0,5000,0,5000;2;21;6;0;;;22;6;0;;',
        layout: { type: 'splitter', source: 'COM7', ports: ['COM21', 'COM22'], baud: 38400 },
      },
    ],
    error: null,
    modifiedAt: now - 30 * DAY,
  };
  const usageView = (): UsageView => ({ ...structuredClone(usage), checkedAt: Date.now() });
  const emitUsage = () => {
    const v = usageView();
    usageListeners.forEach((cb) => cb(v));
    return v;
  };

  const view = (): InventoryView => {
    const rows = devices.map((d) => ({ ...d.row, transport: transportOf(d.row.transport) }));
    const visible = rows.filter((r) => !r.ignored);
    const connected = visible.filter((r) => r.status === 'connected' || r.status === 'problem').length;
    return {
      rows,
      findings: findingsFor(devices),
      summary: {
        total: visible.length,
        connected,
        disconnected: visible.length - connected,
        problems: visible.filter((r) => r.status === 'problem').length,
        warnings: visible.filter((r) => r.severity === 'warning').length,
        ignored: rows.length - visible.length,
        awaiting: visible.filter((r) => r.status === 'awaiting').length,
      },
      scan: {
        platform: 'windows',
        scannedAt: Date.now(),
        durationMs: 42,
        warnings: [],
        capabilities: {
          remembersAbsentDevices: true,
          reservedNumbers: true,
          osTimestamps: true,
          monitoring: 'Plug and Play notifications + serial device map',
        },
        reserved: {
          numbers: [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 16, 20],
          databaseSize: 256,
          source: 'COM port database (ComDB API)',
        },
      },
      storage: { temporary: false, reason: null },
    };
  };

  const emitInventory = () => {
    const v = view();
    listeners.inventory.forEach((cb) => cb(v));
    return v;
  };
  const setUpdate = (patch: Partial<UpdateStatus>) => {
    update = { ...update, ...patch };
    listeners.update.forEach((cb) => cb(update));
    return update;
  };
  const find = (id: number) => {
    const d = devices.find((x) => x.row.deviceId === id);
    if (!d) throw new Error(`device ${id} was not found`);
    return d;
  };

  // ?demo=live: the GPS receiver is plugged in a few seconds after startup.
  if (typeof location !== 'undefined' && new URLSearchParams(location.search).get('demo') === 'live') {
    setTimeout(() => {
      const gps = devices.find((d) => d.row.deviceId === 6);
      if (!gps) return;
      gps.row = { ...gps.row, status: 'connected', lastSeen: Date.now() };
      gps.snapshot = { ...gps.snapshot, presence: 'present' };
      emitInventory();
      const event: InventoryEvent = {
        kind: 'connected',
        deviceId: 6,
        label: 'GPS Receiver',
        port: 'COM16',
        previousPort: null,
        ts: Date.now(),
        initial: false,
      };
      listeners.events.forEach((cb) => cb([event]));
    }, 4000);
  }

  const detail = (id: number): DeviceDetail => {
    const d = find(id);
    const findings = findingsFor(devices).filter((f) => f.deviceIds.includes(id));
    const keys = [] as DeviceDetail['keys'];
    if (d.row.serialNumber && d.row.vid != null && d.row.pid != null) {
      keys.push({
        kind: 'usb-serial',
        value: `${d.row.vid.toString(16).toUpperCase().padStart(4, '0')}:${d.row.pid
          .toString(16)
          .toUpperCase()
          .padStart(4, '0')}:${d.row.serialNumber.toUpperCase().replace(/\s+/g, '_')}:if${d.row.interfaceNumber ?? 0}`,
        strength: 100,
        portable: true,
        description: 'USB serial number',
        firstSeen: d.row.firstSeen,
        lastSeen: d.row.lastSeen ?? now,
      });
    }
    if (d.row.btAddress) {
      keys.push({ kind: 'bluetooth', value: d.row.btAddress, strength: 95, portable: true, description: 'Bluetooth device address', firstSeen: d.row.firstSeen, lastSeen: d.row.lastSeen ?? now });
    }
    if (d.snapshot.system.instanceId) {
      keys.push({
        kind: 'os-device',
        value: d.snapshot.system.instanceId.toUpperCase(),
        strength: d.row.serialNumber ? 90 : 70,
        portable: false,
        description: 'operating-system device instance',
        firstSeen: d.row.firstSeen,
        lastSeen: d.row.lastSeen ?? now,
      });
    }
    if (d.row.vid != null && d.snapshot.usb?.location) {
      keys.push({ kind: 'usb-path', value: `…@${d.snapshot.usb.location}:if${d.row.interfaceNumber ?? 0}`, strength: 60, portable: false, description: 'USB port location', firstSeen: d.row.firstSeen, lastSeen: d.row.lastSeen ?? now });
    }
    const hasSerial = keys.some((k) => k.kind === 'usb-serial');
    const identityNote = hasSerial
      ? 'Recognized by its USB serial number, so it is identified in any USB socket, whatever COM number it receives, and also on other computers after an import.'
      : d.row.transport === 'usb'
        ? 'This device has no unique serial number, so ComInspect recognizes it by the USB socket it is plugged into. If you move it to another socket it will appear as a new entry; use "Same device as…" to link that entry to this one.'
        : d.row.transport === 'bluetooth'
          ? 'Recognized by the Bluetooth address of the paired device.'
          : d.row.transport === 'virtual'
            ? 'Virtual port, recognized by its provider and port name.'
            : 'Recognized by its Windows device instance ID.';
    const connected = d.row.status === 'connected' || d.row.status === 'problem';
    return {
      row: { ...d.row },
      snapshot: d.snapshot,
      snapshotLive: connected,
      recognizedBy: connected ? (hasSerial ? 'usb-serial' : 'os-device') : d.row.status === 'absent_os' ? 'os-device' : null,
      identityNote: identityNote + (connected ? ` Matched in the latest scan by ${hasSerial ? 'USB serial number' : 'operating-system device instance'}.` : ''),
      keys,
      portHistory: d.history.map((h, i) => ({
        port: h.port,
        firstObserved: h.first,
        lastObserved: h.last,
        lastConnected: h.last,
        current: i === 0,
      })),
      events: [
        ...(d.row.previousPort
          ? [{ id: 3, ts: d.row.portChangedAt ?? now, deviceId: id, kind: 'port_changed', port: d.row.port, previousPort: d.row.previousPort, detail: null }]
          : []),
        { id: 2, ts: (d.row.lastSeen ?? now) - 3 * 3600_000, deviceId: id, kind: 'connected', port: d.row.port, previousPort: null, detail: null },
        { id: 1, ts: d.row.firstSeen, deviceId: id, kind: 'new_device', port: d.history[d.history.length - 1]?.port ?? d.row.port, previousPort: null, detail: 'detected at startup' },
      ],
      hints: d.hints,
      findings,
      mergeCandidates: devices
        .filter((o) => o.row.deviceId !== id && o.row.transport === d.row.transport && (o.row.vid === d.row.vid || d.row.vid == null))
        .filter((o) => !(connected && (o.row.status === 'connected' || o.row.status === 'problem')))
        .map((o) => ({
          deviceId: o.row.deviceId,
          label: o.row.nickname ?? o.row.deviceLabel,
          port: o.row.port,
          status: o.row.status,
          lastSeen: o.row.lastSeen,
          reason: o.row.vid != null ? 'Same USB device type' : 'Same connection type',
        })),
      importedFrom: null,
    };
  };

  const appInfo = (): AppInfo => ({
    version: APP_VERSION,
    platform: 'windows',
    arch: 'x86_64',
    identifier: 'io.github.kk4oda.cominspect',
    dataDir: 'C:\\Users\\ham\\AppData\\Local\\io.github.kk4oda.cominspect',
    dbPath: 'C:\\Users\\ham\\AppData\\Local\\io.github.kk4oda.cominspect\\inventory.db',
    logDir: 'C:\\Users\\ham\\AppData\\Local\\io.github.kk4oda.cominspect\\logs',
    backupsDir: 'C:\\Users\\ham\\AppData\\Local\\io.github.kk4oda.cominspect\\backups',
    schemaVersion: 1,
    deviceCount: devices.length,
    keyCount: devices.length * 3,
    eventCount: 214,
    storage: { temporary: false, reason: null },
    databaseBackup: null,
    recoveredFile: null,
    migrationsApplied: [],
    startup: { version: APP_VERSION, updatedFrom: null, firstRun: false, recovery: null, previousVersion: null },
    repository: 'https://github.com/KK4ODA/ComInspect',
    updates: update,
  });

  return {
    kind: 'mock',
    getInventory: () => delay(view()),
    refresh: () => delay(emitInventory(), 250),
    getDeviceDetail: (id) => delay(detail(id), 30),
    updateIdentity: (id, patch) => {
      const d = find(id);
      d.row = { ...d.row, ...Object.fromEntries(Object.entries(patch).map(([k, v]) => [k, v === '' ? null : v])) };
      return delay(emitInventory(), 30);
    },
    setIgnored: (id, ignored) => {
      const d = find(id);
      d.row = { ...d.row, ignored };
      return delay(emitInventory());
    },
    mergeDevices: (target, source) => {
      devices = devices.filter((d) => d.row.deviceId !== source);
      void target;
      return delay(emitInventory());
    },
    forgetDevice: (id) => {
      devices = devices.filter((d) => d.row.deviceId !== id);
      return delay(emitInventory());
    },
    exportInventory: () => delay('C:\\Users\\ham\\Documents\\serial-port-inventory.json', 300),
    importInventory: () =>
      delay({ matched: 2, created: 1, unchanged: 6, sameMachine: false, sourceHostname: 'SHACK-PC', sourceOs: 'windows' }, 300),
    getAppInfo: () => delay(appInfo()),
    appReady: () => delay(undefined),
    getUiPrefs: () => delay(prefs),
    setUiPrefs: (p) => {
      prefs = p;
      return delay(undefined);
    },
    log: async (level, message) => {
      console[level === 'error' ? 'error' : level === 'warn' ? 'warn' : 'log'](`[ui] ${message}`);
    },
    openLocation: () => delay(undefined),
    openLink: () => delay(undefined),
    getUpdateStatus: () => delay(update),
    checkForUpdates: async () => {
      setUpdate({ phase: 'checking', error: null });
      await delay(null, 700);
      return setUpdate({
        phase: 'available',
        lastChecked: Date.now(),
        available: {
          version: NEXT_VERSION,
          date: new Date(now - DAY).toISOString(),
          notes: '- Improved Bluetooth port identification\n- Added CAT-port labels for more Yaesu radios\n- Fixed device matching for FTDI dual-channel interfaces',
          manualInstall: null,
        },
      });
    },
    installUpdate: async (onEvent: (e: DownloadEvent) => void) => {
      const total = 6_400_000;
      setUpdate({ phase: 'downloading', downloaded: 0, total });
      for (let done = 0; done <= total; done += 800_000) {
        await delay(null, 120);
        onEvent({ event: 'progress', data: { downloaded: done, total } });
        setUpdate({ downloaded: done });
      }
      onEvent({ event: 'installing' });
      setUpdate({ phase: 'installing' });
      await delay(null, 600);
      onEvent({ event: 'finished' });
      setUpdate({ phase: 'ready_to_restart' });
    },
    reinstallVersion: async () => {
      throw new Error('Reinstalling is not available in the preview.');
    },
    setAutoUpdateCheck: (enabled) => delay(setUpdate({ autoCheck: enabled })),
    setUpdateChannel: (channel) => delay(setUpdate({ channel, available: null, phase: 'idle' })),
    restartApp: () => delay(undefined),
    listBackups: () =>
      delay([
        { path: 'backups/inventory-manual-20260920-101500.db', fileName: 'inventory-manual-20260920-101500.db', size: 98_304, modified: now - 6 * DAY, schema: 1 },
      ]),
    createBackup: () => delay('backups/inventory-manual.db'),
    restoreBackup: () => delay(view()),
    probeCatalog: () =>
      delay({
        probes: [
          { protocol: 'kenwood_id', label: 'ASCII CAT: read radio ID (ID;)', description: 'Kenwood-style text CAT.', defaultBaud: 0, defaultStopBits: 1 },
          { protocol: 'kenwood_frequency', label: 'ASCII CAT: read VFO A frequency (FA;)', description: 'Kenwood-style text CAT frequency query.', defaultBaud: 0, defaultStopBits: 1 },
          { protocol: 'icom_id', label: 'CI-V: read transceiver ID', description: 'Icom CI-V protocol.', defaultBaud: 0, defaultStopBits: 1 },
          { protocol: 'icom_frequency', label: 'CI-V: read operating frequency', description: 'Icom CI-V frequency query.', defaultBaud: 0, defaultStopBits: 1 },
          { protocol: 'yaesu_legacy_frequency', label: 'Legacy Yaesu 5-byte CAT: read frequency and mode', description: 'Older Yaesu binary CAT.', defaultBaud: 0, defaultStopBits: 2 },
        ],
        icomAddresses: [
          [0x94, 'IC-7300'],
          [0x98, 'IC-7610'],
          [0xa2, 'IC-9700'],
          [0xa4, 'IC-705'],
        ],
        maxPttMs: 3000,
      }),
    openTest: (port) =>
      delay(
        port === 'COM7'
          ? { port, outcome: 'in_use' as const, message: 'The port is in use by another program (Access is denied.).', lines: null, users: [], elapsedMs: 3 }
          : { port, outcome: 'opened' as const, message: 'The port opened successfully and was closed again. No data was sent.', lines: { cts: true, dsr: false, dcd: false, ri: false }, users: [], elapsedMs: 12 },
        400,
      ),
    catQuery: (port, _settings, protocol) =>
      delay(
        protocol === 'kenwood_id'
          ? { port, outcome: 'opened' as const, baudRate: 38400, triedRates: [38400], sentHex: '49 44 3B', receivedHex: '49 44 30 37 36 31 3B', receivedText: 'ID0761;', recognized: true, summary: 'Radio answered with ID 0761 (listed as Yaesu FTDX10). Detected at 38400 baud.', elapsedMs: 84 }
          : { port, outcome: 'opened' as const, baudRate: 115200, triedRates: [38400, 9600, 4800, 19200, 57600, 115200], sentHex: '46 41 3B', receivedHex: '', receivedText: '', recognized: false, summary: 'No valid reply at any common baud rate (tried 38400, 9600, 4800, 19200, 57600, 115200). Check that this is the CAT port and that CAT is enabled on the radio.', elapsedMs: 2700 },
        700,
      ),
    pttTest: (port, line, durationMs) =>
      delay({ port, outcome: 'opened' as const, line, heldMs: durationMs, message: `${line.toUpperCase()} was asserted for ${durationMs} ms and released. If the radio transmitted, this is its PTT port and line.` }, durationMs),
    getPortUsage: () => delay(usageView()),
    watchPort: (port, deviceId, thenOpen) => {
      const entry = usage.ports[port];
      if (!entry) return Promise.reject(new Error(`${port} is not connected.`));
      if (entry.state === 'free') return Promise.reject(new Error(`${port} is already free.`));
      if (deviceId != null) {
        if (thenOpen) programs.set(deviceId, thenOpen);
        else programs.delete(deviceId);
      }
      const armedAt = Date.now();
      entry.watch = { armedAt, thenOpen };
      // The demo program shuts down, then lets go of the port.
      setTimeout(() => {
        const current = usage.ports[port];
        if (!current?.watch || current.watch.armedAt !== armedAt || !current.holders) return;
        current.holders = current.holders.map((h) => ({ ...h, exiting: true }));
        emitUsage();
      }, 2500);
      setTimeout(() => {
        const current = usage.ports[port];
        if (!current?.watch || current.watch.armedAt !== armedAt) return;
        const who = current.holders?.map((h) => h.description ?? h.processName).join(' and ') ?? 'The program';
        const label = devices.find((d) => d.row.port === port)?.row.nickname ?? port;
        const program = current.watch.thenOpen?.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? null;
        usage.ports[port] = { state: 'free', since: Date.now(), sinceExact: true, watch: null };
        delete demoHolders[port];
        emitUsage();
        const seconds = Math.round((Date.now() - armedAt) / 1000);
        releasedListeners.forEach((cb) =>
          cb({
            port,
            message: `${label} (${port}) is free: ${who} released it after ${seconds} s.${program ? ` Starting ${program}.` : ''}`,
            started: program,
            error: null,
          }),
        );
      }, 6000);
      return delay(usageView());
    },
    unwatchPort: (port) => {
      const entry = usage.ports[port];
      if (entry) entry.watch = null;
      return delay(usageView());
    },
    getWatchProgram: (deviceId) => delay(programs.get(deviceId) ?? null),
    getVspe: () => delay(vspe),
    chooseVspeFile: () => delay({ ...vspe, chosen: true, file: 'C:\\Users\\ham\\Documents\\Shack.vspe' }, 200),
    useVspeAutostart: () => delay(vspe),
    pickProgram: () => delay('C:\\VarAC\\VarAC.exe', 200),
    onUsageUpdated: async (cb): Promise<Unlisten> => {
      usageListeners.add(cb);
      return () => usageListeners.delete(cb);
    },
    onUsageReleased: async (cb): Promise<Unlisten> => {
      releasedListeners.add(cb);
      return () => releasedListeners.delete(cb);
    },
    onInventoryUpdated: async (cb): Promise<Unlisten> => {
      listeners.inventory.add(cb);
      return () => listeners.inventory.delete(cb);
    },
    onInventoryEvents: async (cb): Promise<Unlisten> => {
      listeners.events.add(cb);
      return () => listeners.events.delete(cb);
    },
    onUpdateStatus: async (cb): Promise<Unlisten> => {
      listeners.update.add(cb);
      return () => listeners.update.delete(cb);
    },
  };
}
