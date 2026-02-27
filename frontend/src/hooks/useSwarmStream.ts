import { useCallback, useMemo } from 'react';
import { useJsonPatchWsStream } from '@/hooks/useJsonPatchWsStream';
import type { SwarmOverview } from 'shared/types';

interface SwarmStreamData {
  swarm_overview: SwarmOverview | null;
}

/**
 * WebSocket-based swarm overview stream.
 * Connects to /api/tasks/{taskId}/swarm/ws and receives real-time patches.
 */
export function useSwarmStream(taskId: string | undefined) {
  const endpoint = useMemo(
    () => (taskId ? `/api/tasks/${taskId}/swarm/ws` : undefined),
    [taskId]
  );

  const initialData = useCallback(
    (): SwarmStreamData => ({ swarm_overview: null }),
    []
  );

  const { data, isConnected, isInitialized, error } =
    useJsonPatchWsStream<SwarmStreamData>(endpoint, !!taskId, initialData);

  const overview = data?.swarm_overview ?? null;

  return {
    swarm: overview,
    agents: overview?.agents ?? [],
    successions: overview?.successions ?? [],
    dependencies: overview?.dependencies ?? [],
    isConnected,
    isInitialized,
    error,
  };
}
