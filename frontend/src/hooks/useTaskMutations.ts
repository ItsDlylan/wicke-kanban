import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigateWithSearch } from '@/hooks';
import { tasksApi } from '@/lib/api';
import { paths } from '@/lib/paths';
import { taskRelationshipsKeys } from '@/hooks/useTaskRelationships';
import { workspaceSummaryKeys } from '@/components/ui-new/hooks/useWorkspaces';
import type {
  CreateTask,
  CreateAndStartTaskRequest,
  Task,
  TaskWithAttemptStatus,
  UpdateTask,
} from 'shared/types';
import { taskKeys } from './useTask';

export function useTaskMutations(projectId?: string) {
  const queryClient = useQueryClient();
  const navigate = useNavigateWithSearch();

  const invalidateQueries = (taskId?: string) => {
    queryClient.invalidateQueries({ queryKey: taskKeys.all });
    if (taskId) {
      queryClient.invalidateQueries({ queryKey: taskKeys.byId(taskId) });
    }
  };

  const createTask = useMutation({
    mutationFn: (data: CreateTask) => tasksApi.create(data),
    onSuccess: (createdTask: Task) => {
      invalidateQueries();
      // Invalidate parent's relationships cache if this is a subtask
      if (createdTask.parent_workspace_id) {
        queryClient.invalidateQueries({
          queryKey: taskRelationshipsKeys.byAttempt(
            createdTask.parent_workspace_id
          ),
        });
      }
      // Epics don't have attempts — stay on the current page (e.g. planning board)
      if (projectId && createdTask.task_type !== 'epic') {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err, variables) => {
      console.error('[useTaskMutations.createTask] Failed to create task:', {
        err,
        taskData: variables,
      });
    },
  });

  const createAndStart = useMutation({
    mutationFn: (data: CreateAndStartTaskRequest) =>
      tasksApi.createAndStart(data),
    onSuccess: (createdTask: TaskWithAttemptStatus) => {
      invalidateQueries();
      // Invalidate parent's relationships cache if this is a subtask
      if (createdTask.parent_workspace_id) {
        queryClient.invalidateQueries({
          queryKey: taskRelationshipsKeys.byAttempt(
            createdTask.parent_workspace_id
          ),
        });
      }
      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err, variables) => {
      console.error(
        '[useTaskMutations.createAndStart] Failed to create and start task:',
        { err, taskData: variables }
      );
    },
  });

  const updateTask = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: UpdateTask }) =>
      tasksApi.update(taskId, data),
    onSuccess: (updatedTask: Task) => {
      invalidateQueries(updatedTask.id);
    },
    onError: (err, variables) => {
      console.error('[useTaskMutations.updateTask] Failed to update task:', {
        err,
        taskId: variables.taskId,
      });
    },
  });

  const deleteTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.delete(taskId),
    onSuccess: (_: unknown, taskId: string) => {
      invalidateQueries(taskId);
      // Remove single-task cache entry to avoid stale data flashes
      queryClient.removeQueries({ queryKey: ['task', taskId], exact: true });
      // Invalidate all task relationships caches (safe approach since we don't know parent)
      queryClient.invalidateQueries({ queryKey: taskRelationshipsKeys.all });
      // Invalidate workspace summaries so they refresh with the deleted workspace removed
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
    },
    onError: (err, taskId) => {
      console.error('[useTaskMutations.deleteTask] Failed to delete task:', {
        err,
        taskId,
      });
    },
  });

  return {
    createTask,
    createAndStart,
    updateTask,
    deleteTask,
  };
}
