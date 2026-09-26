import { hex4, naturalCompare } from './format';
import type { PortRow, PortStatus, Purpose, Transport } from './types';

export type StatusFilter = 'all' | 'connected' | 'disconnected';
export type TypeFilter = Extract<Transport, 'usb' | 'bluetooth' | 'virtual'>;
export type PurposeFilter = Extract<Purpose, 'cat' | 'ptt' | 'kiss' | 'unknown'>;

export interface Filters {
  search: string;
  status: StatusFilter;
  types: TypeFilter[];
  purposes: PurposeFilter[];
  showIgnored: boolean;
}

export type SortColumn =
  | 'status'
  | 'port'
  | 'nickname'
  | 'device'
  | 'type'
  | 'vidpid'
  | 'serial'
  | 'purpose'
  | 'lastSeen';

export interface Sort {
  column: SortColumn;
  dir: 1 | -1;
}

export function isConnected(status: PortStatus): boolean {
  return status === 'connected' || status === 'problem';
}

/** Text a row can be found by. */
export function searchText(row: PortRow): string {
  return [
    row.nickname,
    row.port,
    row.portShort,
    row.previousPort,
    row.equipment,
    row.manufacturer,
    row.product,
    row.deviceLabel,
    row.hintLabel,
    row.vid != null ? hex4(row.vid) : null,
    row.pid != null ? hex4(row.pid) : null,
    row.vid != null && row.pid != null ? `${hex4(row.vid)}:${hex4(row.pid)}` : null,
    row.serialNumber,
    row.btAddress,
    row.virtualProvider,
    row.notes,
  ]
    .filter(Boolean)
    .join('\n')
    .toLowerCase();
}

export function matches(row: PortRow, f: Filters): boolean {
  if (row.ignored && !f.showIgnored) return false;
  if (f.status === 'connected' && !isConnected(row.status)) return false;
  if (f.status === 'disconnected' && isConnected(row.status)) return false;
  if (f.types.length && !f.types.includes(row.transport as TypeFilter)) return false;
  if (f.purposes.length) {
    const purpose = row.purpose ?? 'unknown';
    const isCat = purpose === 'cat' || purpose === 'secondary_cat';
    const ok = f.purposes.some((p) => (p === 'cat' ? isCat : p === purpose));
    if (!ok) return false;
  }
  const terms = f.search.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length) {
    const text = searchText(row);
    if (!terms.every((t) => text.includes(t))) return false;
  }
  return true;
}

const STATUS_ORDER: Record<PortStatus, number> = {
  problem: 0,
  connected: 1,
  absent_os: 2,
  absent: 3,
  awaiting: 4,
};

/** Compares two values with empty values last, whatever the direction. */
function cmp(a: string | number | null, b: string | number | null, dir: 1 | -1): number {
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  const r = typeof a === 'number' && typeof b === 'number' ? a - b : naturalCompare(String(a), String(b));
  return r * dir;
}

export function compareRows(a: PortRow, b: PortRow, sort: Sort): number {
  const d = sort.dir;
  let r = 0;
  switch (sort.column) {
    case 'status':
      r = (STATUS_ORDER[a.status] - STATUS_ORDER[b.status]) * d;
      break;
    case 'port':
      r = cmp(a.port, b.port, d);
      break;
    case 'nickname':
      r = cmp(a.nickname, b.nickname, d);
      break;
    case 'device':
      r = cmp(a.deviceLabel, b.deviceLabel, d);
      break;
    case 'type':
      r = cmp(a.transport, b.transport, d);
      break;
    case 'vidpid':
      r = cmp(
        a.vid != null ? a.vid * 65536 + (a.pid ?? 0) : null,
        b.vid != null ? b.vid * 65536 + (b.pid ?? 0) : null,
        d,
      );
      break;
    case 'serial':
      r = cmp(a.serialNumber, b.serialNumber, d);
      break;
    case 'purpose':
      r = cmp(a.purpose, b.purpose, d);
      break;
    case 'lastSeen': {
      // Most recent first in the default direction; connected devices are "now".
      const la = isConnected(a.status) ? Number.MAX_SAFE_INTEGER : a.lastSeen;
      const lb = isConnected(b.status) ? Number.MAX_SAFE_INTEGER : b.lastSeen;
      r = cmp(la == null ? null : -la, lb == null ? null : -lb, d);
      break;
    }
  }
  if (r === 0) r = cmp(a.port, b.port, 1);
  if (r === 0) r = a.deviceId - b.deviceId;
  return r;
}

export function applyFilters(rows: PortRow[], f: Filters, sort: Sort): PortRow[] {
  return rows.filter((r) => matches(r, f)).sort((a, b) => compareRows(a, b, sort));
}
