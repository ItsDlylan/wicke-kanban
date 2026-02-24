import { cn } from '@/lib/utils';
import { useSwarmData } from '@/hooks/useSwarmData';
import { useWorkspaceContext } from '@/contexts/WorkspaceContext';
import { useLogsPanel } from '@/contexts/LogsPanelContext';
import { SwarmAgentCard } from '@/components/ui-new/swarm/SwarmAgentCard';
import { SwarmSuccessionCard } from '@/components/ui-new/swarm/SwarmSuccessionCard';
import {
  RIGHT_MAIN_PANEL_MODES,
  useWorkspacePanelState,
} from '@/stores/useUiPreferencesStore';

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  pending: { label: 'Pending', className: 'bg-secondary text-low' },
  running: { label: 'Running', className: 'bg-blue-500/20 text-blue-400' },
  completed: {
    label: 'Completed',
    className: 'bg-green-500/20 text-green-400',
  },
  failed: { label: 'Failed', className: 'bg-red-500/20 text-red-400' },
};

interface SwarmPanelContainerProps {
  className?: string;
}

export function SwarmPanelContainer({ className }: SwarmPanelContainerProps) {
  const { workspace, workspaceId } = useWorkspaceContext();
  const { viewProcessInPanel } = useLogsPanel();
  const { setRightMainPanelMode } = useWorkspacePanelState(workspaceId);

  const taskId = workspace?.task_id ?? undefined;
  const { swarm, agents, successions, isLoading, error, cancel } =
    useSwarmData(taskId);

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
          {swarm.routing_decision && (
            <span className="text-xs text-low">{swarm.routing_decision}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-low">
            {completedCount}/{agents.length}
          </span>
          {(swarm.status === 'running' || swarm.status === 'pending') && (
            <button
              className="rounded bg-red-500/20 px-2 py-0.5 text-xs text-red-400 transition-colors hover:bg-red-500/30"
              onClick={cancel}
            >
              Cancel
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
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
