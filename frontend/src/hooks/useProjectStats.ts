import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { ProjectStats } from 'shared/types';
import { useMemo } from 'react';

export function useAllProjectStats() {
  const query = useQuery({
    queryKey: ['projectStats'],
    queryFn: () => projectsApi.getStats(),
    staleTime: 60_000,
    refetchInterval: 300_000,
  });

  const statsMap = useMemo(() => {
    const map = new Map<string, ProjectStats>();
    if (query.data?.stats) {
      for (const s of query.data.stats) {
        map.set(s.project_id, s);
      }
    }
    return map;
  }, [query.data]);

  return { ...query, statsMap };
}
