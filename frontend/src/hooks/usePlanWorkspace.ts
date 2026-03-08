import { useEffect, useState } from 'react';
import { attemptsApi, sessionsApi } from '@/lib/api';
import type { WorkspaceWithSession } from '@/types/attempt';
import { createWorkspaceWithSession } from '@/types/attempt';

/**
 * Fetches the auto-plan workspace + session for a task while plan is generating.
 * Retries periodically until the workspace/session are found or status changes.
 */
export function usePlanWorkspace(
  taskId: string | undefined,
  planStatus: string | null
): WorkspaceWithSession | null {
  const [planWorkspace, setPlanWorkspace] =
    useState<WorkspaceWithSession | null>(null);

  useEffect(() => {
    if (planStatus !== 'generating' || !taskId) {
      setPlanWorkspace(null);
      return;
    }

    let cancelled = false;

    const fetchPlanWorkspace = async () => {
      try {
        const workspaces = await attemptsApi.getAll(taskId);
        if (cancelled) return;
        const ws = workspaces[0];
        if (!ws) return false;
        const sessions = await sessionsApi.getByWorkspace(ws.id);
        if (cancelled) return;
        if (sessions[0]) {
          setPlanWorkspace(createWorkspaceWithSession(ws, sessions[0]));
          return true;
        }
      } catch {
        // Ignore — will retry
      }
      return false;
    };

    // Try immediately, then poll every 2s until found
    let interval: ReturnType<typeof setInterval> | null = null;
    fetchPlanWorkspace().then((found) => {
      if (!found && !cancelled) {
        interval = setInterval(async () => {
          const found = await fetchPlanWorkspace();
          if (found && interval) {
            clearInterval(interval);
            interval = null;
          }
        }, 2000);
      }
    });

    return () => {
      cancelled = true;
      if (interval) clearInterval(interval);
    };
  }, [taskId, planStatus]);

  return planWorkspace;
}
