import { useMutation, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';
import { attemptKeys } from '@/hooks/useAttempt';
import { workspaceSummaryKeys } from '@/components/ui-new/hooks/useWorkspaces';

interface ToggleArchiveParams {
  workspaceId: string;
  archived: boolean;
  nextWorkspaceId?: string | null;
}

interface TogglePinParams {
  workspaceId: string;
  pinned: boolean;
}

interface DeleteWorkspaceParams {
  workspaceId: string;
  nextWorkspaceId?: string | null;
}

interface UseWorkspaceMutationsOptions {
  onArchiveSuccess?: (params: ToggleArchiveParams) => void;
  onDeleteSuccess?: (params: DeleteWorkspaceParams) => void;
}

export function useWorkspaceMutations(options?: UseWorkspaceMutationsOptions) {
  const queryClient = useQueryClient();

  const invalidateQueries = (workspaceId: string) => {
    queryClient.invalidateQueries({
      queryKey: attemptKeys.byId(workspaceId),
    });
  };

  const toggleArchive = useMutation({
    mutationFn: ({ workspaceId, archived }: ToggleArchiveParams) =>
      attemptsApi.update(workspaceId, { archived: !archived }),
    onSuccess: (_, params) => {
      invalidateQueries(params.workspaceId);
      // Invalidate workspace summaries so stats are refreshed
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
      options?.onArchiveSuccess?.(params);
    },
    onError: (err, variables) => {
      console.error(
        '[useWorkspaceMutations.toggleArchive] Failed to toggle workspace archive:',
        { err, workspaceId: variables.workspaceId }
      );
    },
  });

  const togglePin = useMutation({
    mutationFn: ({ workspaceId, pinned }: TogglePinParams) =>
      attemptsApi.update(workspaceId, { pinned: !pinned }),
    onSuccess: (_, { workspaceId }) => {
      invalidateQueries(workspaceId);
      // Invalidate workspace summaries so stats are refreshed
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
    },
    onError: (err, variables) => {
      console.error(
        '[useWorkspaceMutations.togglePin] Failed to toggle workspace pin:',
        { err, workspaceId: variables.workspaceId }
      );
    },
  });

  const deleteWorkspace = useMutation({
    mutationFn: ({ workspaceId }: DeleteWorkspaceParams) =>
      attemptsApi.delete(workspaceId),
    onSuccess: (_, params) => {
      // Remove the deleted workspace from cache
      queryClient.removeQueries({
        queryKey: attemptKeys.byId(params.workspaceId),
      });
      // Invalidate workspace summaries so list is refreshed
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
      options?.onDeleteSuccess?.(params);
    },
    onError: (err, variables) => {
      console.error(
        '[useWorkspaceMutations.deleteWorkspace] Failed to delete workspace:',
        { err, workspaceId: variables.workspaceId }
      );
    },
  });

  return {
    toggleArchive,
    togglePin,
    deleteWorkspace,
  };
}
