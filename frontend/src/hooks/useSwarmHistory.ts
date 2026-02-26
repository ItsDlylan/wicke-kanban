import { useCallback, useEffect, useState } from 'react';
import { swarmsApi } from '@/lib/api';
import type { SwarmSummary } from 'shared/types';

/**
 * Fetches the list of swarms for a task (swarm history).
 * Returns the list and a selectedSwarmId that can override the active swarm view.
 */
export function useSwarmHistory(taskId: string | undefined) {
  const [swarms, setSwarms] = useState<SwarmSummary[]>([]);
  const [selectedSwarmId, setSelectedSwarmId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const fetchHistory = useCallback(async () => {
    if (!taskId) {
      setSwarms([]);
      return;
    }
    setIsLoading(true);
    try {
      const result = await swarmsApi.listForTask(taskId);
      setSwarms(result);
    } catch {
      // Silently fail — history is supplementary
    } finally {
      setIsLoading(false);
    }
  }, [taskId]);

  useEffect(() => {
    fetchHistory();
  }, [fetchHistory]);

  return {
    swarms,
    selectedSwarmId,
    setSelectedSwarmId,
    isLoading,
    refetch: fetchHistory,
  };
}
