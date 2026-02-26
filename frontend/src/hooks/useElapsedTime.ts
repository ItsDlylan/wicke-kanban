import { useEffect, useState } from 'react';
import { formatElapsedDuration } from '@/utils/date';

/**
 * Returns a live-updating formatted duration string.
 * Updates every second when `isRunning` is true; static otherwise.
 */
export function useElapsedTime(
  startDateStr: string | undefined,
  isRunning: boolean
): string | null {
  const [elapsed, setElapsed] = useState<string | null>(() =>
    startDateStr ? formatElapsedDuration(startDateStr) : null
  );

  useEffect(() => {
    if (!startDateStr) {
      setElapsed(null);
      return;
    }

    setElapsed(formatElapsedDuration(startDateStr));

    if (!isRunning) return;

    const interval = setInterval(() => {
      setElapsed(formatElapsedDuration(startDateStr));
    }, 1000);

    return () => clearInterval(interval);
  }, [startDateStr, isRunning]);

  return elapsed;
}
