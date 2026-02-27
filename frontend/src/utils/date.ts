/**
 * Format a date string as "Jan 5, 10:30 AM".
 */
export function formatDateShortWithTime(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/**
 * Format the elapsed duration between a start time and an optional end time (defaults to now).
 * Returns a compact string like "0:45", "5:23", "1:02:15".
 */
export function formatElapsedDuration(start: string, end?: string): string {
  const startMs = new Date(start).getTime();
  const endMs = end ? new Date(end).getTime() : Date.now();
  const totalSecs = Math.max(0, Math.floor((endMs - startMs) / 1000));

  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;

  if (hours > 0) {
    return `${hours}:${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }
  return `${mins}:${String(secs).padStart(2, '0')}`;
}

/**
 * Format a date string as a relative time (e.g., "just now", "5m ago", "2h ago", "3d ago").
 */
export function formatRelativeTime(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSecs = Math.floor(diffMs / 1000);
  const diffMins = Math.floor(diffSecs / 60);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffSecs < 60) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  return `${diffDays}d ago`;
}
