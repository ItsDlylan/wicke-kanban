import { useQuery } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';

export function useWorkspaceDiffs(attemptId?: string, enabled = true) {
  return useQuery({
    queryKey: ['workspaceDiffs', attemptId],
    queryFn: () => attemptsApi.getDiffs(attemptId!),
    enabled: !!attemptId && enabled,
    staleTime: 30_000,
  });
}
