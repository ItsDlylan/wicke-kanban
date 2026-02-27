import { CheckIcon, ClockIcon } from '@phosphor-icons/react';
import { cn } from '@/lib/utils';
import type { SwarmAgent, SwarmAgentDependency } from 'shared/types';

interface SwarmDependencyLabelProps {
  agent: SwarmAgent;
  agents: SwarmAgent[];
  dependencies: SwarmAgentDependency[];
}

export function SwarmDependencyLabel({
  agent,
  agents,
  dependencies,
}: SwarmDependencyLabelProps) {
  // Only show for pending agents with unmet dependencies
  if (agent.status !== 'pending') return null;

  const agentDeps = dependencies.filter((d) => d.agent_id === agent.id);
  if (agentDeps.length === 0) return null;

  const unmetDeps = agentDeps.filter((d) => {
    const depAgent = agents.find((a) => a.id === d.depends_on_agent_id);
    return depAgent && depAgent.status !== 'completed';
  });

  if (unmetDeps.length === 0) return null;

  return (
    <div className="mt-1 flex flex-wrap items-center gap-1 text-xs text-low">
      <span>Depends on:</span>
      {agentDeps.map((dep) => {
        const depAgent = agents.find((a) => a.id === dep.depends_on_agent_id);
        if (!depAgent) return null;
        const isComplete = depAgent.status === 'completed';
        return (
          <span
            key={dep.depends_on_agent_id}
            className={cn(
              'inline-flex items-center gap-0.5 rounded bg-primary px-1 py-0.5',
              isComplete ? 'text-green-400' : 'text-low'
            )}
          >
            {isComplete ? (
              <CheckIcon className="size-3" weight="bold" />
            ) : (
              <ClockIcon className="size-3" />
            )}
            {depAgent.subtask_description.slice(0, 25)}
            {depAgent.subtask_description.length > 25 ? '...' : ''}
          </span>
        );
      })}
    </div>
  );
}
