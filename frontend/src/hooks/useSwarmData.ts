import { useEffect, useState, useCallback } from 'react';
import { swarmsApi } from '@/lib/api';
import { useSwarmStream } from '@/hooks/useSwarmStream';
import type { SwarmOverview } from 'shared/types';

/**
 * Polls the swarm API for the given task, returning reactive swarm data.
 * Used as fallback when WebSocket is not connected.
 */
function useSwarmDataPolling(taskId: string | undefined) {
  const [data, setData] = useState<SwarmOverview | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSwarm = useCallback(async () => {
    if (!taskId) return;
    try {
      const result = await swarmsApi.getByTaskId(taskId);
      setData(result);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch swarm data');
    } finally {
      setIsLoading(false);
    }
  }, [taskId]);

  useEffect(() => {
    if (!taskId) {
      setData(null);
      return;
    }

    setIsLoading(true);
    fetchSwarm();

    const interval = setInterval(() => {
      if (
        data === null ||
        data.status === 'pending' ||
        data.status === 'running'
      ) {
        fetchSwarm();
      }
    }, 3000);

    return () => clearInterval(interval);
  }, [taskId, fetchSwarm, data?.status]);

  return {
    swarm: data,
    agents: data?.agents ?? [],
    successions: data?.successions ?? [],
    dependencies: data?.dependencies ?? [],
    isLoading,
    error,
    refetch: fetchSwarm,
  };
}

/**
 * Primary hook: WS-first with polling fallback.
 * Supports optional swarmId override for viewing historical swarms.
 */
export function useSwarmData(taskId: string | undefined) {
  const wsData = useSwarmStream(taskId);
  const pollData = useSwarmDataPolling(wsData.isConnected ? undefined : taskId);

  // Prefer WebSocket data when connected
  const source = wsData.isConnected ? wsData : pollData;

  const cancel = useCallback(async () => {
    const swarmId = source.swarm?.id;
    if (!swarmId) return;
    try {
      await swarmsApi.cancel(swarmId);
      if (!wsData.isConnected) {
        pollData.refetch();
      }
    } catch (e) {
      throw e instanceof Error ? e : new Error('Failed to cancel swarm');
    }
  }, [source.swarm?.id, wsData.isConnected, pollData]);

  return {
    swarm: source.swarm,
    agents: source.agents,
    successions: source.successions,
    dependencies: source.dependencies,
    isLoading: !wsData.isConnected && pollData.isLoading,
    error: wsData.error ?? pollData.error,
    cancel,
    refetch: pollData.refetch,
    isWebSocket: wsData.isConnected,
  };
}
