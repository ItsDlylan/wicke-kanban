import { useEffect, useState, useCallback } from 'react';
import { swarmsApi } from '@/lib/api';
import type { SwarmOverview } from 'shared/types';

/**
 * Polls the swarm API for the given task, returning reactive swarm data.
 * Polls every 3 seconds while the swarm is active (pending/running).
 */
export function useSwarmData(taskId: string | undefined) {
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

    // Poll while swarm is active
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

  const cancel = useCallback(async () => {
    if (!data) return;
    try {
      await swarmsApi.cancel(data.id);
      fetchSwarm();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to cancel swarm');
    }
  }, [data, fetchSwarm]);

  return {
    swarm: data,
    agents: data?.agents ?? [],
    successions: data?.successions ?? [],
    isLoading,
    error,
    cancel,
    refetch: fetchSwarm,
  };
}
