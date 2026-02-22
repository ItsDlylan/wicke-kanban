import { useState, useCallback } from 'react';
import { attemptsApi, sessionsApi, tasksApi } from '@/lib/api';
import type { Workspace, Session, BaseCodingAgent } from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';
import { createWorkspaceWithSession } from '@/types/attempt';
import { useQueryClient } from '@tanstack/react-query';

interface PlanEditSessionState {
  workspace: Workspace | null;
  session: Session | null;
  isInitializing: boolean;
  error: string | null;
}

interface UsePlanEditSessionOptions {
  taskId: string;
  taskTitle: string;
  taskDescription: string | undefined;
  currentPlan: string | null;
  executor: BaseCodingAgent;
}

function buildPlanEditPrompt(
  title: string,
  description: string | undefined,
  plan: string | null
): string {
  const descBlock = description
    ? `**Description:** ${description}`
    : '**Description:** (none)';
  const planBlock = plan ?? '(no plan generated yet)';

  return `You are helping the user refine an implementation plan for a development task.

## Task
**Title:** ${title}
${descBlock}

## Current Plan
${planBlock}

## Instructions
Help the user iteratively improve this plan. You should:
1. Present the current plan and ask what they'd like to change
2. Read files and search the codebase as needed for accuracy
3. After each revision, present the complete updated plan
4. Ask the user if they want further modifications

IMPORTANT: Do NOT make any code changes. Only use read-only tools.`;
}

export function usePlanEditSession({
  taskId,
  taskTitle,
  taskDescription,
  currentPlan,
  executor,
}: UsePlanEditSessionOptions) {
  const queryClient = useQueryClient();
  const [state, setState] = useState<PlanEditSessionState>({
    workspace: null,
    session: null,
    isInitializing: false,
    error: null,
  });

  const workspaceWithSession: WorkspaceWithSession | null =
    state.workspace && state.session
      ? createWorkspaceWithSession(state.workspace, state.session)
      : null;

  const startSession = useCallback(async () => {
    setState((s) => ({ ...s, isInitializing: true, error: null }));

    try {
      // Create a workspace for the plan editing session
      const workspace = await attemptsApi.create({
        task_id: taskId,
        executor_profile_id: { executor, variant: null },
        repos: [],
      });

      // Create a session within the workspace
      const session = await sessionsApi.create({
        workspace_id: workspace.id,
        executor,
      });

      // Send the initial plan-editing prompt
      await sessionsApi.followUp(session.id, {
        prompt: buildPlanEditPrompt(taskTitle, taskDescription, currentPlan),
        executor_profile_id: { executor, variant: null },
        retry_process_id: null,
        force_when_dirty: null,
        perform_git_reset: null,
      });

      setState({
        workspace,
        session,
        isInitializing: false,
        error: null,
      });
    } catch (err: unknown) {
      const message =
        err instanceof Error
          ? err.message
          : 'Failed to start plan editing session';
      setState((s) => ({
        ...s,
        isInitializing: false,
        error: message,
      }));
    }
  }, [taskId, taskTitle, taskDescription, currentPlan, executor]);

  const savePlan = useCallback(
    async (planText: string) => {
      await tasksApi.updatePlan(taskId, planText);
      // Invalidate task queries so the UI refreshes
      queryClient.invalidateQueries({ queryKey: ['tasks'] });
      queryClient.invalidateQueries({ queryKey: ['task', taskId] });
    },
    [taskId, queryClient]
  );

  const cleanup = useCallback(async () => {
    if (state.workspace) {
      try {
        await attemptsApi.update(state.workspace.id, { archived: true });
      } catch {
        // Best-effort cleanup
      }
    }
    setState({
      workspace: null,
      session: null,
      isInitializing: false,
      error: null,
    });
  }, [state.workspace]);

  return {
    workspace: state.workspace,
    session: state.session,
    workspaceWithSession,
    isInitializing: state.isInitializing,
    error: state.error,
    startSession,
    savePlan,
    cleanup,
  };
}
