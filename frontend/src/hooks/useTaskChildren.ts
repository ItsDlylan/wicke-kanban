import { useQuery } from '@tanstack/react-query';
import { decomposeApi } from '@/lib/api';
import type { ChildTaskWithDeps } from 'shared/types';

export function useTaskChildren(taskId?: string) {
  return useQuery<ChildTaskWithDeps[]>({
    queryKey: ['task-children', taskId],
    queryFn: () => decomposeApi.getChildren(taskId!),
    enabled: !!taskId,
    refetchInterval: 5000,
  });
}
