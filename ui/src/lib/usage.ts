import type { PortHolder, PortUsageView } from './types';

/** "VARA FM", or the executable name without ".exe". */
export function holderName(h: PortHolder): string {
  const d = h.description?.trim();
  if (d) return d;
  return h.processName.replace(/\.exe$/i, '');
}

/** "VARA FM (VARAFM.exe, PID 4312)". */
export function describeHolder(h: PortHolder): string {
  const name = holderName(h);
  const text =
    name.toLowerCase() === h.processName.toLowerCase()
      ? `${name} (PID ${h.pid})`
      : `${name} (${h.processName}, PID ${h.pid})`;
  return h.exiting ? `${text}, shutting down` : text;
}

/** "VARA FM", "VARA FM and WSJT-X", "fldigi, flrig and WSJT-X". */
export function holderNames(holders: PortHolder[]): string {
  const names = holders.map(holderName);
  if (names.length <= 1) return names[0] ?? '';
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

/** A program's name from its path: "C:\VarAC\VarAC.exe" -> "VarAC". */
export function programName(path: string): string {
  const file = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  return file.replace(/\.(exe|lnk|bat|cmd|app|desktop|sh)$/i, '');
}

/** "less than a minute", "5 min", "2 h 5 min", "3 days". */
export function span(ms: number): string {
  const minutes = Math.floor(Math.max(ms, 0) / 60_000);
  if (minutes < 1) return 'less than a minute';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
  const days = Math.floor(hours / 24);
  return days === 1 ? '1 day' : `${days} days`;
}

/** One line for a tooltip: "In use by VARA FM (VARAFM.exe, PID 4312)". */
export function usageSummary(usage: PortUsageView): string {
  switch (usage.state) {
    case 'in_use':
      return `In use by ${(usage.holders ?? []).map(describeHolder).join(', ')}`;
    case 'free':
      return 'Not in use by any program';
    default:
      return `Could not check which programs use this port${usage.reason ? ` (${usage.reason})` : ''}`;
  }
}
