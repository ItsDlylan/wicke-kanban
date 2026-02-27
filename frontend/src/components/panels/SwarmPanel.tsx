import { useState, useCallback, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { useSwarmData } from '@/hooks/useSwarmData';
import { SwarmAgentCard } from '@/components/ui-new/swarm/SwarmAgentCard';
import { SwarmSuccessionCard } from '@/components/ui-new/swarm/SwarmSuccessionCard';
import { SwarmTimeline } from '@/components/ui-new/swarm/SwarmTimeline';
import { formatDateShortWithTime } from '@/utils/date';
import { ChevronDown, ChevronRight } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  pending: {
    label: 'Pending',
    className: 'bg-secondary text-muted-foreground',
  },
  running: { label: 'Running', className: 'bg-blue-500/20 text-blue-400' },
  completed: {
    label: 'Completed',
    className: 'bg-green-500/20 text-green-400',
  },
  failed: { label: 'Failed', className: 'bg-red-500/20 text-red-400' },
  cancelled: {
    label: 'Cancelled',
    className: 'bg-orange-500/20 text-orange-400',
  },
};

const ROUTING_LABELS: Record<string, { label: string; description: string }> = {
  single: {
    label: 'Single Agent',
    description: 'One agent handles the full task',
  },
  single_verifier: {
    label: 'Single + Verifier',
    description: 'One agent with verification checking',
  },
  vs_shallow: {
    label: 'VS Shallow',
    description:
      'Verified succession — one generation of successor if context threshold is reached',
  },
  vs_deep: {
    label: 'VS Deep',
    description: 'Full verified succession swarm with multiple generations',
  },
};

interface SwarmPanelProps {
  taskId: string;
  className?: string;
}

export function SwarmPanel({ taskId, className }: SwarmPanelProps) {
  const { swarm, agents, successions, dependencies, isLoading, error, cancel } =
    useSwarmData(taskId);

  const [isCancelling, setIsCancelling] = useState(false);
  const [isTimelineExpanded, setIsTimelineExpanded] = useState(false);

  useEffect(() => {
    if (swarm?.status === 'cancelled') {
      setIsCancelling(false);
    }
  }, [swarm?.status]);

  const handleCancel = useCallback(async () => {
    setIsCancelling(true);
    try {
      await cancel();
    } catch {
      setIsCancelling(false);
    }
  }, [cancel]);

  if (isLoading && !swarm) {
    return (
      <div
        className={cn(
          'flex items-center justify-center p-4 text-sm text-muted-foreground',
          className
        )}
      >
        Loading swarm data...
      </div>
    );
  }

  if (error) {
    return (
      <div
        className={cn(
          'flex items-center justify-center p-4 text-sm text-destructive',
          className
        )}
      >
        {error}
      </div>
    );
  }

  if (!swarm) {
    return (
      <div
        className={cn(
          'flex items-center justify-center p-4 text-sm text-muted-foreground',
          className
        )}
      >
        No active swarm for this task
      </div>
    );
  }

  const completedCount = agents.filter((a) => a.status === 'completed').length;
  const statusStyle = STATUS_LABELS[swarm.status] ?? STATUS_LABELS.pending;
  const routingInfo = swarm.routing_decision
    ? ROUTING_LABELS[swarm.routing_decision]
    : null;

  return (
    <div className={cn('flex h-full flex-col overflow-hidden', className)}>
      {/* Header */}
      <div className="flex items-center justify-between border-b px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">Swarm</span>
          <span
            className={cn(
              'rounded-full px-1.5 py-0.5 text-xs font-medium',
              statusStyle.className
            )}
          >
            {statusStyle.label}
          </span>
          {swarm.routing_decision && (
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="cursor-help text-xs text-muted-foreground">
                    {routingInfo?.label ?? swarm.routing_decision}
                  </span>
                </TooltipTrigger>
                {routingInfo && (
                  <TooltipContent side="bottom">
                    {routingInfo.description}
                  </TooltipContent>
                )}
              </Tooltip>
            </TooltipProvider>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">
            {completedCount}/{agents.length}
          </span>
          {swarm.status === 'cancelled' && (
            <span className="text-xs text-muted-foreground">
              Cancelled {formatDateShortWithTime(swarm.updated_at)}
            </span>
          )}
          {(swarm.status === 'running' || swarm.status === 'pending') && (
            <button
              className={cn(
                'rounded bg-red-500/20 px-2 py-0.5 text-xs text-red-400 transition-colors',
                isCancelling
                  ? 'cursor-not-allowed opacity-50'
                  : 'hover:bg-red-500/30'
              )}
              onClick={handleCancel}
              disabled={isCancelling}
            >
              {isCancelling ? 'Cancelling...' : 'Cancel'}
            </button>
          )}
        </div>
      </div>

      {/* Agent list */}
      <div className="flex-1 overflow-y-auto p-2">
        <div className="space-y-1.5">
          {agents.map((agent) => (
            <SwarmAgentCard
              key={agent.id}
              agent={agent}
              agents={agents}
              dependencies={dependencies}
            />
          ))}
        </div>

        {/* Successions */}
        {successions.length > 0 && (
          <div className="mt-3">
            <div className="mb-1.5 text-xs font-medium text-muted-foreground">
              Successions
            </div>
            <div className="space-y-1.5">
              {successions.map((succession) => (
                <SwarmSuccessionCard
                  key={succession.id}
                  succession={succession}
                  agents={agents}
                />
              ))}
            </div>
          </div>
        )}

        {/* Timeline */}
        <div className="mt-3">
          <button
            className="flex w-full items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground"
            onClick={() => setIsTimelineExpanded((v) => !v)}
          >
            {isTimelineExpanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
            Timeline
          </button>
          {isTimelineExpanded && (
            <SwarmTimeline
              swarm={swarm}
              agents={agents}
              successions={successions}
            />
          )}
        </div>
      </div>
    </div>
  );
}
