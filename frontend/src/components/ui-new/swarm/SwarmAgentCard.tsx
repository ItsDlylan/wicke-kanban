import { cn } from '@/lib/utils';
import type { SwarmAgent, SwarmAgentStatus } from 'shared/types';

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
  onSelect?: (executionProcessId: string) => void;
}

export function SwarmAgentCard({ agent, onSelect }: SwarmAgentCardProps) {
  const style = STATUS_STYLES[agent.status];
  const contextUsed = Number(agent.context_tokens_used ?? 0);
  const contextWindow = Number(agent.context_window_size ?? 200000);
  const utilization =
    contextWindow > 0 ? Math.min(contextUsed / contextWindow, 1) : 0;

  return (
    <div
      className={cn(
        'rounded border border-white/5 bg-secondary p-2 transition-colors',
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

      {agent.status === 'running' && (
        <div className="mt-1.5">
          <div className="flex items-center justify-between text-xs text-low">
            <span>Context</span>
            <span>{Math.round(utilization * 100)}%</span>
          </div>
          <div className="mt-0.5 h-1 rounded-full bg-primary">
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
          </div>
        </div>
      )}
    </div>
  );
}
