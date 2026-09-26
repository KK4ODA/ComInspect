import type { CatStatus, Category, PortStatus, Purpose, Transport } from './types';

export const PURPOSES: { value: Purpose; label: string }[] = [
  { value: 'cat', label: 'CAT' },
  { value: 'secondary_cat', label: 'Secondary CAT' },
  { value: 'ptt', label: 'PTT' },
  { value: 'cw', label: 'CW' },
  { value: 'programming', label: 'Programming' },
  { value: 'data', label: 'Data' },
  { value: 'kiss', label: 'KISS' },
  { value: 'gps', label: 'GPS' },
  { value: 'control', label: 'Control' },
  { value: 'other', label: 'Other' },
  { value: 'unknown', label: 'Unknown' },
];

export const CATEGORIES: { value: Category; label: string }[] = [
  { value: 'radio_cat', label: 'Radio CAT' },
  { value: 'ptt', label: 'PTT' },
  { value: 'cw_keying', label: 'CW Keying' },
  { value: 'tnc', label: 'TNC' },
  { value: 'kiss', label: 'KISS' },
  { value: 'gps', label: 'GPS' },
  { value: 'rotator', label: 'Rotator' },
  { value: 'amplifier', label: 'Amplifier' },
  { value: 'antenna_tuner', label: 'Antenna tuner' },
  { value: 'antenna_switch', label: 'Antenna switch' },
  { value: 'programming_cable', label: 'Programming cable' },
  { value: 'radio_interface', label: 'Radio interface' },
  { value: 'bluetooth_serial', label: 'Bluetooth serial' },
  { value: 'virtual_serial', label: 'Virtual serial' },
  { value: 'generic_serial', label: 'Generic serial' },
  { value: 'other', label: 'Other' },
];

export const CAT_STATUSES: { value: CatStatus; label: string }[] = [
  { value: 'verified', label: 'Verified CAT' },
  { value: 'not_cat', label: 'Not CAT' },
  { value: 'unknown', label: 'Unknown' },
];

export const TRANSPORTS: Record<Transport, string> = {
  usb: 'USB',
  bluetooth: 'Bluetooth',
  virtual: 'Virtual',
  pci: 'PCI',
  builtin: 'Built-in',
  unknown: 'Unknown',
};

export const STATUS_TEXT: Record<PortStatus, string> = {
  connected: 'Connected',
  problem: 'Connected — driver problem',
  absent_os: 'Disconnected — Windows still reserves this port',
  absent: 'Disconnected — previously seen',
  awaiting: 'Imported — not yet seen on this computer',
};

export const STATUS_SHORT: Record<PortStatus, string> = {
  connected: 'Connected',
  problem: 'Problem',
  absent_os: 'Hidden',
  absent: 'Disconnected',
  awaiting: 'Awaiting',
};

export function purposeLabel(p: Purpose | null | undefined): string {
  return PURPOSES.find((x) => x.value === p)?.label ?? '';
}

export function categoryLabel(c: Category | null | undefined): string {
  return CATEGORIES.find((x) => x.value === c)?.label ?? '';
}

export const KEY_KIND_LABEL: Record<string, string> = {
  'usb-serial': 'USB serial',
  bluetooth: 'Bluetooth',
  'os-device': 'OS instance',
  'usb-path': 'USB socket',
  virtual: 'Virtual',
};

export const EVENT_TEXT: Record<string, string> = {
  new_device: 'First seen',
  connected: 'Connected',
  disconnected: 'Disconnected',
  port_changed: 'Port changed',
  merged: 'Linked',
  imported: 'Imported',
};
