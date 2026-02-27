import { useMemo } from 'react';
import { cn } from '@/lib/utils';
import { formatDateShortWithTime } from '@/utils/date';
import type { Swarm, SwarmAgent, SwarmSuccession } from 'shared/types';

interface TimelineEvent {
  timestamp: string;
  label: string;
  color: 'blue' | 'green' | 'red' | 'yellow' | 'gray';
}

function buildSwarmTimeline(
  swarm: Swarm,
  agents: SwarmAgent[],
  successions: SwarmSuccession[]
): TimelineEvent[] {
  const events: TimelineEvent[] = [];

  events.push({
    timestamp: swarm.created_at,
    label: 'Swarm started',
    color: 'blue',
  });

  for (const agent of agents) {
    events.push({
      timestamp: agent.created_at,
      label: `Agent created: ${agent.subtask_description.slice(0, 60)}${agent.subtask_description.length > 60 ? '...' : ''}`,
      color: 'gray',
    });

    if (
      agent.status === 'completed' ||
      agent.status === 'failed' ||
      agent.status === 'threshold'
    ) {
      const colorMap: Record<string, TimelineEvent['color']> = {
        completed: 'green',
        failed: 'red',
        threshold: 'yellow',
      };
      events.push({
        timestamp: agent.updated_at,
        label: `Agent ${agent.status}: ${agent.subtask_description.slice(0, 40)}${agent.subtask_description.length > 40 ? '...' : ''}`,
        color: colorMap[agent.status] ?? 'gray',
      });
    }
  }

  for (const succession of successions) {
    events.push({
      timestamp: succession.created_at,
      label: 'Succession initiated',
      color: 'yellow',
    });

    if (
      succession.status === 'verified' ||
      succession.status === 'successor_running'
    ) {
      events.push({
        timestamp: succession.updated_at,
        label: `Verification complete (${Math.round((succession.verifier_confidence ?? 0) * 100)}% confidence)`,
        color: 'green',
      });
    } else if (succession.status === 'failed') {
      events.push({
        timestamp: succession.updated_at,
        label: 'Succession verification failed',
        color: 'red',
      });
    }
  }

  if (
    swarm.status === 'completed' ||
    swarm.status === 'failed' ||
    swarm.status === 'cancelled'
  ) {
    const colorMap: Record<string, TimelineEvent['color']> = {
      completed: 'green',
      failed: 'red',
      cancelled: 'yellow',
    };
    events.push({
      timestamp: swarm.updated_at,
      label: `Swarm ${swarm.status}`,
      color: colorMap[swarm.status] ?? 'gray',
    });
  }

  events.sort(
    (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
  );

  return events;
}

const DOT_COLORS: Record<TimelineEvent['color'], string> = {
  blue: 'bg-blue-400',
  green: 'bg-green-400',
  red: 'bg-red-400',
  yellow: 'bg-yellow-400',
  gray: 'bg-white/20',
};

interface SwarmTimelineProps {
  swarm: Swarm;
  agents: SwarmAgent[];
  successions: SwarmSuccession[];
}

export function SwarmTimeline({
  swarm,
  agents,
  successions,
}: SwarmTimelineProps) {
  const events = useMemo(
    () => buildSwarmTimeline(swarm, agents, successions),
    [swarm, agents, successions]
  );

  if (events.length === 0) return null;

  return (
    <div className="px-3 py-2">
      <div className="relative ml-1.5 border-l border-white/10 pl-4">
        {events.map((event, i) => (
          <div key={i} className="relative mb-2 last:mb-0">
            <div
              className={cn(
                'absolute -left-[21px] top-1 size-2 rounded-full',
                DOT_COLORS[event.color]
              )}
            />
            <div className="text-xs text-low">
              {formatDateShortWithTime(event.timestamp)}
            </div>
            <div className="text-xs text-normal">{event.label}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
