import { WarningIcon } from '@phosphor-icons/react';
import { cn } from '@/lib/utils';
import { useElapsedTime } from '@/hooks/useElapsedTime';
import { formatElapsedDuration } from '@/utils/date';
import { SwarmDependencyLabel } from '@/components/ui-new/swarm/SwarmDependencyLabel';
import type {
  SwarmAgent,
  SwarmAgentDependency,
  SwarmAgentStatus,
} from 'shared/types';

const STATUS_STYLES: Record<
  SwarmAgentStatus,
  { bg: string; text: string; label: string }
> = {
  pending: { bg: 'bg-secondary', text: 'text-low', label: 'Pending' },
  running: { bg: 'bg-blue-500/20', text: 'text-blue-400', label: 'Running' },
  completed: {
    bg: 'bg-green-500/20',
    text: 'text-green-400',
    label: 'Completed',
  },
  failed: { bg: 'bg-red-500/20', text: 'text-red-400', label: 'Failed' },
  threshold: {
    bg: 'bg-yellow-500/20',
    text: 'text-yellow-400',
    label: 'Threshold',
  },
};

interface SwarmAgentCardProps {
  agent: SwarmAgent;
  agents?: SwarmAgent[];
  dependencies?: SwarmAgentDependency[];
  onSelect?: (executionProcessId: string) => void;
}

export function SwarmAgentCard({
  agent,
  agents = [],
  dependencies = [],
  onSelect,
}: SwarmAgentCardProps) {
  const style = STATUS_STYLES[agent.status];
  const contextUsed = Number(agent.context_tokens_used ?? 0);
  const contextWindow = Number(agent.context_window_size ?? 200000);
  const utilization =
    contextWindow > 0 ? Math.min(contextUsed / contextWindow, 1) : 0;
  const thresholdPct = agent.context_threshold * 100;

  const isRunning = agent.status === 'running';
  const isTerminal =
    agent.status === 'completed' ||
    agent.status === 'failed' ||
    agent.status === 'threshold';

  // Duration tracking
  const liveElapsed = useElapsedTime(agent.created_at, isRunning);
  const staticElapsed =
    isTerminal && agent.created_at
      ? formatElapsedDuration(agent.created_at, agent.updated_at)
      : null;
  const elapsed = isRunning ? liveElapsed : staticElapsed;

  // Show context bar for running + terminal statuses (not pending)
  const showContextBar = agent.status !== 'pending';

  return (
    <div
      className={cn(
        'rounded border bg-secondary p-2 transition-colors',
        agent.status === 'threshold'
          ? 'border-yellow-500/40 animate-pulse'
          : 'border-white/5',
        agent.execution_process_id && 'cursor-pointer hover:bg-panel'
      )}
      onClick={() => {
        if (agent.execution_process_id && onSelect) {
          onSelect(agent.execution_process_id);
        }
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-sm text-normal">
          {agent.subtask_description}
        </span>
        <div className="flex shrink-0 items-center gap-1.5">
          {elapsed && (
            <span className="font-mono text-xs text-low">{elapsed}</span>
          )}
          {Number(agent.generation) > 1 && (
            <span className="text-xs text-low">
              Gen {Number(agent.generation)}
            </span>
          )}
          <span
            className={cn(
              'rounded-full px-1.5 py-0.5 text-xs font-medium',
              style.bg,
              style.text
            )}
          >
            {style.label}
          </span>
        </div>
      </div>

      {/* Dependency labels */}
      <SwarmDependencyLabel
        agent={agent}
        agents={agents}
        dependencies={dependencies}
      />

      {/* Context utilization bar */}
      {showContextBar && (
        <div className="mt-1.5">
          <div className="flex items-center justify-between text-xs text-low">
            <span>Context</span>
            <span>{Math.round(utilization * 100)}%</span>
          </div>
          <div className="relative mt-0.5 h-1 rounded-full bg-primary">
            <div
              className={cn(
                'h-full rounded-full transition-all duration-500',
                utilization >= 0.8
                  ? 'bg-red-400'
                  : utilization >= 0.6
                    ? 'bg-yellow-400'
                    : 'bg-blue-400'
              )}
              style={{ width: `${utilization * 100}%` }}
            />
            {/* Threshold marker */}
            <div
              className="absolute top-0 h-full w-px bg-yellow-500/60"
              style={{ left: `${thresholdPct}%` }}
              title={`Threshold: ${Math.round(thresholdPct)}%`}
            />
          </div>
        </div>
      )}

      {/* Failure context */}
      {agent.status === 'failed' && (
        <div className="mt-1.5 flex items-center justify-between">
          <div className="flex items-center gap-1 text-xs text-red-400">
            <WarningIcon className="size-3" weight="fill" />
            <span>Execution failed</span>
          </div>
          {agent.execution_process_id && onSelect && (
            <button
              className="text-xs text-low underline-offset-2 hover:underline"
              onClick={(e) => {
                e.stopPropagation();
                onSelect(agent.execution_process_id!);
              }}
            >
              View logs
            </button>
          )}
        </div>
      )}
    </div>
  );
}
