import { describe, expect, it } from 'vitest';
import type { PortRow, VspeDevice, VspeView } from './types';
import { describeDevice, missingFromConfig, portList, sourceLabel, splitterOf, treeRows, vspeLinks } from './vspe';

let nextId = 1;
function row(p: Partial<PortRow>): PortRow {
  return {
    deviceId: nextId++,
    uuid: 'u',
    status: 'connected',
    port: 'COM1',
    portShort: 'COM1',
    previousPort: null,
    portChangedAt: null,
    nickname: null,
    equipment: null,
    category: null,
    purpose: null,
    catStatus: 'unknown',
    notes: null,
    ignored: false,
    deviceLabel: 'Device',
    hintLabel: null,
    manufacturer: null,
    product: null,
    transport: 'usb',
    vid: null,
    pid: null,
    serialNumber: null,
    interfaceNumber: null,
    btAddress: null,
    virtualProvider: null,
    firstSeen: 0,
    lastSeen: 0,
    lastSeenSource: 'app',
    severity: null,
    findingCount: 0,
    ...p,
  };
}

const virt = (port: string, p: Partial<PortRow> = {}) =>
  row({ port, portShort: port, transport: 'virtual', virtualProvider: 'VSPE', ...p });

const devices: VspeDevice[] = [
  { kind: 'Splitter', settings: '', layout: { type: 'splitter', source: 'COM5', ports: ['COM10', 'COM8'], baud: 9600 } },
  { kind: 'Pair', settings: '21;22;0', layout: { type: 'pair', ports: ['COM21', 'COM22'] } },
  { kind: 'TcpServer', settings: '', layout: { type: 'network', port: 'COM5', protocol: 'TCP server', address: 'port 5555' } },
  { kind: 'Bridge', settings: '?', layout: null },
];

describe('VSPE links', () => {
  const links = vspeLinks(devices);

  it('indexes every role by port', () => {
    expect(links.splitOf.get('COM8')?.source).toBe('COM5');
    expect(links.sharedBy.get('COM5')).toHaveLength(1);
    expect(links.roles.get('COM21')).toEqual([{ kind: 'pair', peer: 'COM22' }]);
    expect(links.roles.get('COM5')).toEqual([{ kind: 'network', protocol: 'TCP server', address: 'port 5555' }]);
  });

  it('only virtual ports belong to a splitter', () => {
    expect(splitterOf(virt('COM8'), links)?.source).toBe('COM5');
    expect(splitterOf(row({ port: 'COM8' }), links)).toBeNull();
  });

  it("nests a splitter's virtual ports under their source port, in the splitter's order", () => {
    const digirig = row({ port: 'COM5', portShort: 'COM5' });
    const other = row({ port: 'COM3' });
    const c8 = virt('COM8');
    const c10 = virt('com10');
    const tree = treeRows([c8, other, digirig, c10], links);
    expect(tree.map((t) => [t.row.port, t.depth, t.last])).toEqual([
      ['COM3', 0, false],
      ['COM5', 0, false],
      ['com10', 1, false],
      ['COM8', 1, true],
    ]);
  });

  it('leaves virtual ports in place when their source port is not listed', () => {
    const c8 = virt('COM8');
    const tree = treeRows([row({ port: 'COM3' }), c8], links);
    expect(tree.map((t) => [t.row.port, t.depth])).toEqual([
      ['COM3', 0],
      ['COM8', 0],
    ]);
  });

  it('prefers the connected device when several had the source port name', () => {
    const old = row({ port: 'COM5', status: 'absent' });
    const now = row({ port: 'COM5' });
    const tree = treeRows([old, now, virt('COM8')], links);
    expect(tree.map((t) => [t.row.deviceId, t.depth])).toEqual([
      [old.deviceId, 0],
      [now.deviceId, 0],
      [tree[2].row.deviceId, 1],
    ]);
  });

  it('lists ports for sentences', () => {
    expect(portList(['COM8'])).toBe('COM8');
    expect(portList(['COM8', 'COM10'])).toBe('COM8 and COM10');
    expect(portList(['COM8', 'COM10', 'COM11'])).toBe('COM8, COM10 and COM11');
  });
});

describe('VSPE configuration', () => {
  const view = (partial: Partial<VspeView> = {}): VspeView => ({
    source: { mode: 'autostart' },
    autostartPath: 'C:\\ProgramData\\Eterlogic\\VSPE\\autostart.vspe',
    file: 'C:\\ProgramData\\Eterlogic\\VSPE\\autostart.vspe',
    devices,
    error: null,
    modifiedAt: 0,
    ...partial,
  });

  it('describes every device', () => {
    expect(devices.map(describeDevice)).toEqual([
      'Splitter: COM5 shared as COM10 and COM8 (9600 baud)',
      'Pair: COM21 ↔ COM22',
      'TCP server: COM5 on port 5555',
      'Bridge: not understood by ComInspect yet',
    ]);
  });

  it('names connected VSPE ports the file does not mention', () => {
    // A splitter added after the startup configuration was saved (COM18 -> COM26, COM27).
    const rows = [
      virt('COM8'),
      virt('COM26'),
      virt('COM27', { status: 'absent' }),
      virt('COM30', { virtualProvider: 'com0com' }),
      row({ port: 'COM18', transport: 'bluetooth' }),
    ];
    expect(missingFromConfig(rows, view())).toEqual(['COM26']);
    expect(missingFromConfig(rows, view({ file: null }))).toEqual([]);
    expect(missingFromConfig(rows, view({ error: 'unreadable' }))).toEqual([]);
  });

  it('says where the configuration comes from', () => {
    expect(sourceLabel(view())).toBe("VSPE's startup configuration");
    expect(sourceLabel(view({ source: { mode: 'file', path: 'C:\\VSPE\\Shack.vspe' }, file: 'C:\\VSPE\\Shack.vspe' }))).toBe('Shack.vspe');
    expect(
      sourceLabel(
        view({ source: { mode: 'folder', path: 'C:\\Users\\ham\\VSPE' }, file: 'C:\\Users\\ham\\VSPE\\Lenovo_X1_092726.vspe' }),
      ),
    ).toBe('Lenovo_X1_092726.vspe (newest in VSPE)');
    expect(sourceLabel(view({ source: { mode: 'folder', path: 'D:\\Radio' }, file: null }))).toBe('the newest file in Radio');
  });
});
