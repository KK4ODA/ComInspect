const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "Now", "5 min ago", "Yesterday", "3 weeks ago" … */
export function relativeTime(ts: number | null | undefined, now: number = Date.now()): string {
  if (ts == null) return '—';
  const diff = now - ts;
  if (diff < 0) return 'Just now';
  if (diff < MINUTE) return 'Just now';
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} min ago`;
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  if (ts >= startOfToday.getTime()) return 'Today';
  if (ts >= startOfToday.getTime() - DAY) return 'Yesterday';
  const days = Math.floor(diff / DAY);
  if (days < 7) return `${days} days ago`;
  if (days < 30) {
    const weeks = Math.floor(days / 7);
    return weeks === 1 ? '1 week ago' : `${weeks} weeks ago`;
  }
  if (days < 365) {
    const months = Math.floor(days / 30);
    return months === 1 ? '1 month ago' : `${months} months ago`;
  }
  const years = Math.floor(days / 365);
  return years === 1 ? '1 year ago' : `${years} years ago`;
}

export function dateTime(ts: number | null | undefined): string {
  if (ts == null) return '—';
  return new Date(ts).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function timeOfDay(ts: number | null | undefined): string {
  if (ts == null) return '—';
  return new Date(ts).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export function hex4(value: number | null | undefined): string {
  return value == null ? '' : value.toString(16).toUpperCase().padStart(4, '0');
}

export function vidPid(vid: number | null | undefined, pid: number | null | undefined): string {
  return vid == null || pid == null ? '—' : `${hex4(vid)}:${hex4(pid)}`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** Splits text into chunks for natural ordering (COM2 < COM10). */
export function naturalCompare(a: string, b: string): number {
  const ax = a.toLowerCase().match(/\d+|\D+/g) ?? [];
  const bx = b.toLowerCase().match(/\d+|\D+/g) ?? [];
  for (let i = 0; i < Math.min(ax.length, bx.length); i++) {
    const x = ax[i];
    const y = bx[i];
    if (x === y) continue;
    const nx = /^\d/.test(x);
    const ny = /^\d/.test(y);
    if (nx && ny) return Number(x) - Number(y);
    if (nx !== ny) return nx ? -1 : 1;
    return x < y ? -1 : 1;
  }
  return ax.length - bx.length;
}

export function isRecent(ts: number | null | undefined, now: number, days = 7): boolean {
  return ts != null && now - ts < days * DAY;
}
