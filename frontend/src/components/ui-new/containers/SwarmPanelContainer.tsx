import { useState, useCallback, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { useSwarmData } from '@/hooks/useSwarmData';
import { useWorkspaceContext } from '@/contexts/WorkspaceContext';
import { useLogsPanel } from '@/contexts/LogsPanelContext';
import { SwarmAgentCard } from '@/components/ui-new/swarm/SwarmAgentCard';
import { SwarmSuccessionCard } from '@/components/ui-new/swarm/SwarmSuccessionCard';
import { SwarmTimeline } from '@/components/ui-new/swarm/SwarmTimeline';
import { Tooltip } from '@/components/ui-new/primitives/Tooltip';
import { CollapsibleSectionHeader } from '@/components/ui-new/primitives/CollapsibleSectionHeader';
import {
  RIGHT_MAIN_PANEL_MODES,
  useWorkspacePanelState,
} from '@/stores/useUiPreferencesStore';
import { formatDateShortWithTime } from '@/utils/date';

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  pending: { label: 'Pending', className: 'bg-secondary text-low' },
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

interface SwarmPanelContainerProps {
  className?: string;
}

export function SwarmPanelContainer({ className }: SwarmPanelContainerProps) {
  const { workspace, workspaceId } = useWorkspaceContext();
  const { viewProcessInPanel } = useLogsPanel();
  const { setRightMainPanelMode } = useWorkspacePanelState(workspaceId);

  const taskId = workspace?.task_id ?? undefined;
  const { swarm, agents, successions, dependencies, isLoading, error, cancel } =
    useSwarmData(taskId);

  const [isCancelling, setIsCancelling] = useState(false);

  // Reset cancelling state when swarm status changes to cancelled
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

  const handleAgentSelect = (executionProcessId: string) => {
    viewProcessInPanel(executionProcessId);
    setRightMainPanelMode(RIGHT_MAIN_PANEL_MODES.LOGS);
  };

  if (!taskId) {
    return (
      <div className={cn('flex items-center justify-center p-4', className)}>
        <span className="text-sm text-low">No workspace selected</span>
      </div>
    );
  }

  if (isLoading && !swarm) {
    return (
      <div className={cn('flex items-center justify-center p-4', className)}>
        <span className="text-sm text-low">Loading swarm data...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className={cn('flex items-center justify-center p-4', className)}>
        <span className="text-sm text-red-400">{error}</span>
      </div>
    );
  }

  if (!swarm) {
    return (
      <div className={cn('flex items-center justify-center p-4', className)}>
        <span className="text-sm text-low">No active swarm for this task</span>
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
      <div className="flex items-center justify-between border-b border-white/5 px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-high">Swarm</span>
          <span
            className={cn(
              'rounded-full px-1.5 py-0.5 text-xs font-medium',
              statusStyle.className
            )}
          >
            {statusStyle.label}
          </span>
          {swarm.routing_decision &&
            (routingInfo ? (
              <Tooltip content={routingInfo.description}>
                <span className="cursor-help text-xs text-low">
                  {routingInfo.label}
                </span>
              </Tooltip>
            ) : (
              <span className="text-xs text-low">{swarm.routing_decision}</span>
            ))}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-low">
            {completedCount}/{agents.length}
          </span>
          {swarm.status === 'cancelled' && (
            <span className="text-xs text-low">
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
              onSelect={handleAgentSelect}
            />
          ))}
        </div>

        {/* Successions */}
        {successions.length > 0 && (
          <div className="mt-3">
            <div className="mb-1.5 text-xs font-medium text-low">
              Successions
            </div>
            <div className="space-y-1.5">
              {successions.map((succession) => (
                <SwarmSuccessionCard
                  key={succession.id}
                  succession={succession}
                  agents={agents}
                  onAgentSelect={handleAgentSelect}
                />
              ))}
            </div>
          </div>
        )}

        {/* Timeline */}
        <CollapsibleSectionHeader
          persistKey="swarm:timeline"
          title="Timeline"
          defaultExpanded={false}
          className="mt-3"
        >
          <SwarmTimeline
            swarm={swarm}
            agents={agents}
            successions={successions}
          />
        </CollapsibleSectionHeader>
      </div>
    </div>
  );
}
