import type { PortRow, VspeDevice, VspeView } from './types';

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

/** One line per device: "Splitter: COM5 shared as COM8 and COM10 (9600 baud)". */
export function describeDevice(d: VspeDevice): string {
  const l = d.layout;
  if (!l) return `${d.kind}: not understood by ComInspect yet`;
  switch (l.type) {
    case 'splitter':
      return `Splitter: ${l.source} shared as ${portList(l.ports)}${l.baud ? ` (${l.baud} baud)` : ''}`;
    case 'connector':
      return `Connector: ${l.port}`;
    case 'pair':
      return `Pair: ${l.ports.join(' ↔ ')}`;
    case 'redirector':
      return `Redirector: ${l.ports.join(' ↔ ')}`;
    case 'network':
      return `${l.protocol}: ${l.port} on ${l.address}`;
  }
}

/** Every port the configuration mentions (upper case). */
export function mentionedPorts(devices: VspeDevice[]): Set<string> {
  const ports = new Set<string>();
  for (const { layout } of devices) {
    if (!layout) continue;
    if (layout.type === 'splitter') {
      ports.add(key(layout.source));
      layout.ports.forEach((p) => ports.add(key(p)));
    } else if (layout.type === 'connector' || layout.type === 'network') {
      ports.add(key(layout.port));
    } else {
      layout.ports.forEach((p) => ports.add(key(p)));
    }
  }
  return ports;
}

/**
 * Connected VSPE ports the configuration doesn't mention: a sign that VSPE's
 * setup changed since the file was saved. Empty when no file was read.
 */
export function missingFromConfig(rows: PortRow[], view: VspeView | null): string[] {
  if (!view?.file || view.error) return [];
  const known = mentionedPorts(view.devices);
  return rows
    .filter((r) => r.virtualProvider === 'VSPE' && r.port && (r.status === 'connected' || r.status === 'problem'))
    .map((r) => r.port!)
    .filter((p) => !known.has(key(p)))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
}

/** The last part of a Windows or Unix path. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** "VSPE's startup configuration", "Shack.vspe" or "Lenovo_X1_092726.vspe (newest in VSPE)". */
export function sourceLabel(view: VspeView): string {
  const file = view.file ? baseName(view.file) : null;
  switch (view.source.mode) {
    case 'autostart':
      return "VSPE's startup configuration";
    case 'file':
      return file ?? baseName(view.source.path);
    case 'folder':
      return file ? `${file} (newest in ${baseName(view.source.path)})` : `the newest file in ${baseName(view.source.path)}`;
  }
}
