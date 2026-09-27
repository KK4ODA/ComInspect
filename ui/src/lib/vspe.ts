import type { PortRow, VspeDevice } from './types';

/** A VSPE Splitter: `source` shared through the virtual `ports`. */
export interface SplitterLink {
  source: string;
  ports: string[];
  baud: number | null;
}

/** What else VSPE does with a port. */
export type VspeRole =
  | { kind: 'pair' | 'redirector'; peer: string }
  | { kind: 'connector' }
  | { kind: 'network'; protocol: string; address: string };

/** How VSPE links ports. Keys are upper-case port names. */
export interface PortLinks {
  /** Virtual port -> the Splitter that creates it. */
  splitOf: Map<string, SplitterLink>;
  /** Source port -> the Splitters sharing it. */
  sharedBy: Map<string, SplitterLink[]>;
  /** Port -> its other VSPE roles. */
  roles: Map<string, VspeRole[]>;
}

const key = (port: string) => port.toUpperCase();

function push<T>(map: Map<string, T[]>, port: string, value: T): void {
  const list = map.get(key(port));
  if (list) list.push(value);
  else map.set(key(port), [value]);
}

export function vspeLinks(devices: VspeDevice[]): PortLinks {
  const links: PortLinks = { splitOf: new Map(), sharedBy: new Map(), roles: new Map() };
  for (const { layout } of devices) {
    switch (layout?.type) {
      case 'splitter':
        push(links.sharedBy, layout.source, layout);
        for (const port of layout.ports) links.splitOf.set(key(port), layout);
        break;
      case 'pair':
      case 'redirector': {
        const [a, b] = layout.ports;
        if (a && b) {
          push(links.roles, a, { kind: layout.type, peer: b });
          push(links.roles, b, { kind: layout.type, peer: a });
        }
        break;
      }
      case 'connector':
        push(links.roles, layout.port, { kind: 'connector' });
        break;
      case 'network':
        push(links.roles, layout.port, { kind: 'network', protocol: layout.protocol, address: layout.address });
        break;
    }
  }
  return links;
}

/** The Splitter that creates this row's port, if any. */
export function splitterOf(row: PortRow, links: PortLinks): SplitterLink | null {
  return row.port && row.transport === 'virtual' ? (links.splitOf.get(key(row.port)) ?? null) : null;
}

export interface TreeRow {
  row: PortRow;
  /** 1 for a Splitter's virtual port shown under its source port. */
  depth: 0 | 1;
  /** The last virtual port under its source port. */
  last: boolean;
}

/**
 * Rows in display order, with each Splitter's virtual ports right after the
 * port they share (in the Splitter's order). A virtual port whose source port
 * is not in `rows` stays where it is.
 */
export function treeRows(rows: PortRow[], links: PortLinks): TreeRow[] {
  // The row for each port name, preferring a connected device.
  const byPort = new Map<string, PortRow>();
  for (const r of rows) {
    if (!r.port) continue;
    const current = byPort.get(key(r.port));
    const connected = (x: PortRow) => x.status === 'connected' || x.status === 'problem';
    if (!current || (!connected(current) && connected(r))) byPort.set(key(r.port), r);
  }
  const children = new Map<number, { row: PortRow; order: number }[]>();
  const nested = new Set<number>();
  for (const r of rows) {
    const split = splitterOf(r, links);
    if (!split) continue;
    const parent = byPort.get(key(split.source));
    if (!parent || parent.deviceId === r.deviceId || splitterOf(parent, links)) continue;
    const order = split.ports.findIndex((p) => key(p) === key(r.port!));
    const list = children.get(parent.deviceId) ?? [];
    list.push({ row: r, order });
    children.set(parent.deviceId, list);
    nested.add(r.deviceId);
  }
  const out: TreeRow[] = [];
  for (const r of rows) {
    if (nested.has(r.deviceId)) continue;
    out.push({ row: r, depth: 0, last: false });
    const kids = (children.get(r.deviceId) ?? []).sort((a, b) => a.order - b.order || a.row.deviceId - b.row.deviceId);
    kids.forEach((k, i) => out.push({ row: k.row, depth: 1, last: i === kids.length - 1 }));
  }
  return out;
}

/** "COM8", "COM8 and COM10", "COM8, COM10 and COM11". */
export function portList(ports: string[]): string {
  if (ports.length <= 1) return ports[0] ?? '';
  return `${ports.slice(0, -1).join(', ')} and ${ports[ports.length - 1]}`;
}
