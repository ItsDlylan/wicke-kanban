import { useMemo, useState, useRef, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Gauge } from 'lucide-react';
import { cn } from '@/lib/utils';
import { usageApi, type ClaudeUsageData } from '@/lib/api';

interface WindowUsage {
  label: string;
  percentage: number;
  used: number;
  limit: number;
  resetAt: string | null;
}

function extractWindowUsage(
  usage: Record<string, unknown> | null
): WindowUsage[] {
  if (!usage) return [];

  const windows: WindowUsage[] = [];

  const candidates = usage.usage_windows ?? usage.windows ?? usage;
  const obj =
    typeof candidates === 'object' && candidates !== null
      ? (candidates as Record<string, unknown>)
      : {};

  for (const [key, value] of Object.entries(obj)) {
    if (typeof value !== 'object' || value === null) continue;
    const win = value as Record<string, unknown>;

    const used =
      typeof win.used === 'number'
        ? win.used
        : typeof win.tokens_used === 'number'
          ? win.tokens_used
          : null;
    const limit =
      typeof win.limit === 'number'
        ? win.limit
        : typeof win.tokens_limit === 'number'
          ? win.tokens_limit
          : null;
    const percentage =
      typeof win.percentage === 'number'
        ? win.percentage
        : typeof win.utilization === 'number'
          ? win.utilization * 100
          : used !== null && limit !== null && limit > 0
            ? (used / limit) * 100
            : null;

    if (percentage !== null) {
      const label = formatWindowLabel(key);
      windows.push({
        label,
        percentage: Math.min(100, Math.round(percentage)),
        used: used ?? 0,
        limit: limit ?? 0,
        resetAt:
          typeof win.reset_at === 'string'
            ? win.reset_at
            : typeof win.resets_at === 'string'
              ? win.resets_at
              : null,
      });
    }
  }

  return windows.sort((a, b) => {
    const order = ['5h', '7d', 'daily', 'weekly', 'monthly'];
    const aIdx = order.findIndex((o) =>
      a.label.toLowerCase().includes(o.toLowerCase())
    );
    const bIdx = order.findIndex((o) =>
      b.label.toLowerCase().includes(o.toLowerCase())
    );
    return (aIdx === -1 ? 99 : aIdx) - (bIdx === -1 ? 99 : bIdx);
  });
}

function formatWindowLabel(key: string): string {
  return key.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function getStatusColor(pct: number): string {
  if (pct >= 80) return 'text-red-500';
  if (pct >= 50) return 'text-yellow-500';
  return 'text-green-500';
}

function getBarColor(pct: number): string {
  if (pct >= 80) return 'bg-red-500';
  if (pct >= 50) return 'bg-yellow-500';
  return 'bg-green-500';
}

function formatResetTime(resetAt: string | null): string | null {
  if (!resetAt) return null;
  try {
    const date = new Date(resetAt);
    const now = new Date();
    const diffMs = date.getTime() - now.getTime();
    if (diffMs <= 0) return 'soon';
    const diffMin = Math.round(diffMs / 60_000);
    if (diffMin < 60) return `${diffMin}m`;
    const diffH = Math.round(diffMin / 60);
    if (diffH < 24) return `${diffH}h`;
    const diffD = Math.round(diffH / 24);
    return `${diffD}d`;
  } catch {
    return null;
  }
}

export function UsageIndicator() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const { data } = useQuery<ClaudeUsageData | null>({
    queryKey: ['claude-usage'],
    queryFn: () => usageApi.get(),
    refetchInterval: 30_000,
  });

  const windows = useMemo(
    () => extractWindowUsage(data?.usage ?? null),
    [data?.usage]
  );

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  if (!data || !data.configured) {
    return null;
  }

  const primary = windows[0];
  const secondary = windows[1];
  const primaryPct = primary?.percentage ?? 0;

  if (windows.length === 0 && !data.error) {
    return null;
  }

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={cn(
          'flex items-center gap-1 rounded px-1.5 py-1 text-xs',
          'hover:bg-muted transition-colors',
          data.error ? 'text-yellow-500' : getStatusColor(primaryPct)
        )}
        aria-label={`Claude usage: ${primaryPct}%`}
      >
        <Gauge className="h-3.5 w-3.5" />
        {!data.error && windows.length > 0 && (
          <span className="tabular-nums">
            {primary && `${primary.percentage}%`}
            {secondary && (
              <span className="opacity-60">/{secondary.percentage}%</span>
            )}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-full z-[10000] mt-1 w-64 rounded-md border bg-popover p-3 shadow-md">
          <div className="space-y-3">
            <h4 className="text-sm font-medium text-foreground">
              Claude Usage
            </h4>

            {data.error && (
              <div className="rounded bg-yellow-500/10 px-2 py-1.5 text-xs text-yellow-500">
                {data.error}
              </div>
            )}

            {windows.length > 0 && (
              <div className="space-y-2.5">
                {windows.map((win) => (
                  <div key={win.label} className="space-y-1">
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-foreground">{win.label}</span>
                      <span
                        className={cn(
                          'font-medium',
                          getStatusColor(win.percentage)
                        )}
                      >
                        {win.percentage}%
                      </span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-border">
                      <div
                        className={cn(
                          'h-full rounded-full transition-all duration-300',
                          getBarColor(win.percentage)
                        )}
                        style={{ width: `${win.percentage}%` }}
                      />
                    </div>
                    {win.resetAt && (
                      <div className="text-[10px] text-muted-foreground">
                        Resets in {formatResetTime(win.resetAt)}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}

            {data.last_updated_at && (
              <div className="text-[10px] text-muted-foreground">
                Updated{' '}
                {new Date(data.last_updated_at).toLocaleTimeString()}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
