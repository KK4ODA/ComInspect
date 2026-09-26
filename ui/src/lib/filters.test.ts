import { describe, expect, it } from 'vitest';
import { applyFilters, matches, type Filters } from './filters';
import { naturalCompare, relativeTime, vidPid } from './format';
import type { PortRow } from './types';

function row(p: Partial<PortRow>): PortRow {
  return {
    deviceId: 1,
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

const none: Filters = { search: '', status: 'all', types: [], purposes: [], showIgnored: false };

describe('filters', () => {
  const rows = [
    row({ deviceId: 1, port: 'COM10', nickname: 'FTDX10 CAT Enhanced', purpose: 'cat', vid: 0x10c4, pid: 0xea70, serialNumber: '01A2B3C4' }),
    row({ deviceId: 2, port: 'COM2', nickname: 'GPS', purpose: 'gps', status: 'absent_os' }),
    row({ deviceId: 3, port: 'COM3', transport: 'virtual', purpose: 'kiss', notes: 'VARA side of the pair' }),
    row({ deviceId: 4, port: 'COM4', ignored: true }),
    row({ deviceId: 5, port: 'COM5', purpose: 'secondary_cat', transport: 'bluetooth' }),
  ];

  it('hides ignored rows unless requested', () => {
    expect(applyFilters(rows, none, { column: 'port', dir: 1 }).map((r) => r.deviceId)).toEqual([2, 3, 5, 1]);
    expect(applyFilters(rows, { ...none, showIgnored: true }, { column: 'port', dir: 1 })).toHaveLength(5);
  });

  it('filters by status, type and purpose', () => {
    expect(rows.filter((r) => matches(r, { ...none, status: 'disconnected' })).map((r) => r.deviceId)).toEqual([2]);
    expect(rows.filter((r) => matches(r, { ...none, types: ['virtual', 'bluetooth'] })).map((r) => r.deviceId)).toEqual([3, 5]);
    // CAT includes secondary CAT.
    expect(rows.filter((r) => matches(r, { ...none, purposes: ['cat'] })).map((r) => r.deviceId)).toEqual([1, 5]);
  });

  it('searches nickname, VID:PID, serial and notes', () => {
    const find = (search: string) => rows.filter((r) => matches(r, { ...none, search })).map((r) => r.deviceId);
    expect(find('ftdx10 enhanced')).toEqual([1]);
    expect(find('10c4:ea70')).toEqual([1]);
    expect(find('01a2b3')).toEqual([1]);
    expect(find('vara')).toEqual([3]);
    expect(find('com2')).toEqual([2]);
  });

  it('sorts naturally and keeps empty values last', () => {
    const sorted = applyFilters(rows, none, { column: 'nickname', dir: -1 }).map((r) => r.nickname);
    expect(sorted).toEqual(['GPS', 'FTDX10 CAT Enhanced', null, null]);
    const byStatus = applyFilters(rows, none, { column: 'status', dir: 1 }).map((r) => r.deviceId);
    expect(byStatus).toEqual([3, 5, 1, 2]);
  });
});

describe('format', () => {
  it('orders port names naturally', () => {
    expect(['COM10', 'COM2', 'COM1'].sort(naturalCompare)).toEqual(['COM1', 'COM2', 'COM10']);
  });

  it('formats VID:PID', () => {
    expect(vidPid(0x10c4, 0xea60)).toBe('10C4:EA60');
    expect(vidPid(null, null)).toBe('—');
  });

  it('describes relative times', () => {
    const now = new Date('2026-09-26T15:00:00').getTime();
    expect(relativeTime(now - 30_000, now)).toBe('Just now');
    expect(relativeTime(now - 5 * 60_000, now)).toBe('5 min ago');
    expect(relativeTime(new Date('2026-09-25T20:00:00').getTime(), now)).toBe('Yesterday');
    expect(relativeTime(now - 21 * 86_400_000, now)).toBe('3 weeks ago');
    expect(relativeTime(null, now)).toBe('—');
  });
});
