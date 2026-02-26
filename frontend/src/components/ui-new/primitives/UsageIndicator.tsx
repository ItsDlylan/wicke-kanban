import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { GaugeIcon } from '@phosphor-icons/react';
import { cn } from '@/lib/utils';
import { usageApi, type ClaudeUsageData } from '@/lib/api';
import {
  Popover,
  PopoverTrigger,
  PopoverContent,
} from '@/components/ui-new/primitives/Popover';

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

  // Handle the standard shape: { daily: { used, limit, ... }, ... }
  // or nested under a key like usage_windows, etc.
  const candidates = usage.usage_windows ?? usage.windows ?? usage;
  const obj =
    typeof candidates === 'object' && candidates !== null
      ? (candidates as Record<string, unknown>)
      : {};

  for (const [key, value] of Object.entries(obj)) {
    if (typeof value !== 'object' || value === null) continue;
    const win = value as Record<string, unknown>;

    // Look for common patterns
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

  // Sort by window duration (shorter windows first)
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
  if (pct >= 80) return 'text-error';
  if (pct >= 50) return 'text-warning';
  return 'text-success';
}

function getBarColor(pct: number): string {
  if (pct >= 80) return 'bg-error';
  if (pct >= 50) return 'bg-warning';
  return 'bg-success';
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
  const { data } = useQuery<ClaudeUsageData | null>({
    queryKey: ['claude-usage'],
    queryFn: () => usageApi.get(),
    refetchInterval: 30_000,
  });

  const windows = useMemo(
    () => extractWindowUsage(data?.usage ?? null),
    [data?.usage]
  );

  // Hide when not configured or no data loaded yet
  if (!data || !data.configured) {
    return null;
  }

  // Find the primary (shortest window) percentage for the trigger display
  const primary = windows[0];
  const secondary = windows[1];
  const primaryPct = primary?.percentage ?? 0;

  // If we have no parseable windows and no error, hide
  if (windows.length === 0 && !data.error) {
    return null;
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            'flex items-center gap-quarter rounded-sm px-quarter py-px text-xs',
            'hover:bg-panel transition-colors',
            data.error ? 'text-warning' : getStatusColor(primaryPct)
          )}
          aria-label={`Claude usage: ${primaryPct}%`}
        >
          <GaugeIcon className="size-icon-sm" weight="bold" />
          {!data.error && windows.length > 0 && (
            <span className="tabular-nums">
              {primary && `${primary.percentage}%`}
              {secondary && (
                <span className="text-low">/{secondary.percentage}%</span>
              )}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent side="bottom" align="end" className="w-72">
        <div className="space-y-base">
          <h4 className="text-sm font-medium text-normal">Claude Usage</h4>

          {data.error && (
            <div className="rounded-sm bg-warning/10 p-half text-xs text-warning">
              {data.error}
            </div>
          )}

          {windows.length > 0 && (
            <div className="space-y-half">
              {windows.map((win) => (
                <div key={win.label} className="space-y-quarter">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-normal">{win.label}</span>
                    <span
                      className={cn(
                        'font-medium',
                        getStatusColor(win.percentage)
                      )}
                    >
                      {win.percentage}%
                    </span>
                  </div>
                  <div className="h-1.5 w-full rounded-full bg-border overflow-hidden">
                    <div
                      className={cn(
                        'h-full rounded-full transition-all duration-300',
                        getBarColor(win.percentage)
                      )}
                      style={{ width: `${win.percentage}%` }}
                    />
                  </div>
                  {win.resetAt && (
                    <div className="text-[10px] text-low">
                      Resets in {formatResetTime(win.resetAt)}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {data.last_updated_at && (
            <div className="text-[10px] text-low">
              Updated {new Date(data.last_updated_at).toLocaleTimeString()}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
