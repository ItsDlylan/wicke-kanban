import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import RepoBranchSelector from '@/components/tasks/RepoBranchSelector';
import { ExecutorProfileSelector } from '@/components/settings';
import { useRepoBranchSelection, useProjectRepos } from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useUserSystem } from '@/components/ConfigProvider';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { decomposeApi } from '@/lib/api';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  ChildTaskWithDeps,
  ExecutorProfileId,
  TaskStatus,
} from 'shared/types';
import { Loader2 } from 'lucide-react';

export interface SprintPlannerDialogProps {
  taskId: string;
  taskTitle: string;
}

function StatusBadge({ status }: { status: TaskStatus }) {
  const colors: Record<string, string> = {
    todo: 'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-200',
    done: 'bg-green-200 text-green-800 dark:bg-green-800 dark:text-green-200',
    ralph: 'bg-blue-200 text-blue-800 dark:bg-blue-800 dark:text-blue-200',
    inreview:
      'bg-yellow-200 text-yellow-800 dark:bg-yellow-800 dark:text-yellow-200',
  };
  return (
    <span
      className={`px-1.5 py-0.5 text-xs rounded ${colors[status] ?? 'bg-gray-200 text-gray-700'}`}
    >
      {status}
    </span>
  );
}

const SprintPlannerDialogImpl = NiceModal.create<SprintPlannerDialogProps>(
  ({ taskId, taskTitle }) => {
    const modal = useModal();
    const { t } = useTranslation('tasks');
    const { projectId } = useProject();
    const { profiles, config } = useUserSystem();
    const queryClient = useQueryClient();

    const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
    const [isDecomposing, setIsDecomposing] = useState(false);
    const [isStarting, setIsStarting] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [selectedProfile, setSelectedProfile] =
      useState<ExecutorProfileId | null>(null);

    const {
      data: children,
      isLoading: isLoadingChildren,
      refetch: refetchChildren,
    } = useQuery({
      queryKey: ['task-children', taskId],
      queryFn: () => decomposeApi.getChildren(taskId),
      enabled: modal.visible,
    });

    const { data: projectRepos = [], isLoading: isLoadingRepos } =
      useProjectRepos(projectId, { enabled: modal.visible });

    const {
      configs: repoBranchConfigs,
      isLoading: isLoadingBranches,
      setRepoBranch,
      getWorkspaceRepoInputs,
    } = useRepoBranchSelection({
      repos: projectRepos,
      enabled: modal.visible && projectRepos.length > 0,
    });

    const effectiveProfile =
      selectedProfile ?? config?.executor_profile ?? null;
    const hasChildren = (children?.length ?? 0) > 0;

    // Group children by sprint workspace
    const sprintGroups = useMemo(() => {
      if (!children) return new Map<string | null, ChildTaskWithDeps[]>();
      const groups = new Map<string | null, ChildTaskWithDeps[]>();
      for (const child of children) {
        const key = child.sprint_workspace_id ?? null;
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key)!.push(child);
      }
      return groups;
    }, [children]);

    // Unassigned children (no sprint workspace)
    const unassigned = sprintGroups.get(null) ?? [];

    // Active sprints
    const activeSprintEntries = useMemo(() => {
      return Array.from(sprintGroups.entries()).filter(
        ([key]) => key !== null
      ) as [string, ChildTaskWithDeps[]][];
    }, [sprintGroups]);

    useEffect(() => {
      if (!modal.visible) {
        setSelectedIds(new Set());
        setError(null);
        setIsDecomposing(false);
        setIsStarting(false);
      }
    }, [modal.visible]);

    const handleDecompose = async () => {
      setIsDecomposing(true);
      setError(null);
      try {
        await decomposeApi.decompose(taskId);
        await refetchChildren();
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Decomposition failed');
      } finally {
        setIsDecomposing(false);
      }
    };

    const handleToggle = (id: string) => {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
    };

    const handleSelectAll = () => {
      setSelectedIds(new Set(unassigned.map((c) => c.id)));
    };

    const handleSelectReady = () => {
      setSelectedIds(
        new Set(unassigned.filter((c) => c.is_ready).map((c) => c.id))
      );
    };

    const handleStartSprint = async () => {
      if (selectedIds.size === 0 || !effectiveProfile) return;
      setIsStarting(true);
      setError(null);
      try {
        const repos = getWorkspaceRepoInputs();
        await decomposeApi.startSprint(taskId, {
          task_ids: Array.from(selectedIds),
          executor_profile_id: effectiveProfile,
          repos,
        });
        // Invalidate task queries to update board
        queryClient.invalidateQueries({ queryKey: ['tasks'] });
        queryClient.invalidateQueries({ queryKey: ['task-children'] });
        await refetchChildren();
        setSelectedIds(new Set());
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to start sprint');
      } finally {
        setIsStarting(false);
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) modal.hide();
    };

    const allBranchesSelected = repoBranchConfigs.every(
      (c) => c.targetBranch !== null
    );

    const canStart = Boolean(
      selectedIds.size > 0 &&
        effectiveProfile &&
        allBranchesSelected &&
        projectRepos.length > 0 &&
        !isStarting
    );

    // Build a lookup for child titles by ID (for showing dependency names)
    const childTitleMap = useMemo(() => {
      const m = new Map<string, string>();
      children?.forEach((c) => m.set(c.id, c.title));
      return m;
    }, [children]);

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={handleOpenChange}
        className="max-w-3xl w-[92vw]"
      >
        <DialogContent
          className="max-h-[85vh] flex flex-col"
          onKeyDownCapture={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              modal.hide();
            }
          }}
        >
          <DialogHeader>
            <DialogTitle>Sprint Planner: {taskTitle}</DialogTitle>
          </DialogHeader>

          <div className="flex-1 overflow-auto space-y-4 min-h-0">
            {/* Decompose button */}
            {!hasChildren && !isLoadingChildren && (
              <div className="flex justify-center py-6">
                <Button onClick={handleDecompose} disabled={isDecomposing}>
                  {isDecomposing ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Decomposing...
                    </>
                  ) : (
                    'Decompose into Stories'
                  )}
                </Button>
              </div>
            )}

            {isLoadingChildren && (
              <div className="flex justify-center py-6">
                <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
              </div>
            )}

            {/* Unassigned children table */}
            {unassigned.length > 0 && (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-medium">
                    Unassigned Tasks ({unassigned.length})
                  </h3>
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleSelectAll}
                    >
                      Select All
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleSelectReady}
                    >
                      Select Ready
                    </Button>
                  </div>
                </div>

                <div className="border rounded-md divide-y">
                  {unassigned.map((child) => (
                    <label
                      key={child.id}
                      className="flex items-start gap-3 p-3 hover:bg-muted/50 cursor-pointer"
                    >
                      <Checkbox
                        checked={selectedIds.has(child.id)}
                        onCheckedChange={() => handleToggle(child.id)}
                        className="mt-0.5"
                      />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="font-medium text-sm truncate">
                            {child.title}
                          </span>
                          <StatusBadge status={child.status} />
                          {child.is_ready ? (
                            <span className="px-1.5 py-0.5 text-xs rounded bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300">
                              ready
                            </span>
                          ) : (
                            <span className="px-1.5 py-0.5 text-xs rounded bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-300">
                              blocked
                            </span>
                          )}
                        </div>
                        {child.dependencies.length > 0 && (
                          <div className="text-xs text-muted-foreground mt-1">
                            Depends on:{' '}
                            {child.dependencies
                              .map(
                                (depId) =>
                                  childTitleMap.get(depId) ?? depId.slice(0, 8)
                              )
                              .join(', ')}
                          </div>
                        )}
                      </div>
                    </label>
                  ))}
                </div>
              </div>
            )}

            {/* Existing sprints */}
            {activeSprintEntries.length > 0 && (
              <div className="space-y-2">
                <h3 className="text-sm font-medium">Active Sprints</h3>
                {activeSprintEntries.map(([wsId, tasks]) => {
                  const done = tasks.filter((t) => t.status === 'done').length;
                  return (
                    <div key={wsId} className="border rounded-md p-3">
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm font-medium">
                          Sprint {wsId.slice(0, 8)}
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {done}/{tasks.length} done
                        </span>
                      </div>
                      <div className="space-y-1">
                        {tasks.map((task) => (
                          <div
                            key={task.id}
                            className="flex items-center gap-2 text-sm"
                          >
                            <StatusBadge status={task.status} />
                            <span className="truncate">{task.title}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}

            {/* Sprint config */}
            {hasChildren && unassigned.length > 0 && (
              <div className="space-y-3 border rounded-md p-3">
                <h3 className="text-sm font-medium">Sprint Configuration</h3>
                {profiles && (
                  <ExecutorProfileSelector
                    profiles={profiles}
                    selectedProfile={effectiveProfile}
                    onProfileSelect={setSelectedProfile}
                    showLabel={true}
                  />
                )}
                <RepoBranchSelector
                  configs={repoBranchConfigs}
                  onBranchChange={setRepoBranch}
                  isLoading={isLoadingBranches || isLoadingRepos}
                  showLabel={true}
                />
              </div>
            )}

            {error && <div className="text-sm text-destructive">{error}</div>}
          </div>

          <DialogFooter>
            <div className="flex items-center justify-between w-full">
              <span className="text-sm text-muted-foreground">
                {selectedIds.size > 0 &&
                  `${selectedIds.size} task${selectedIds.size > 1 ? 's' : ''} selected`}
              </span>
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => modal.hide()}>
                  {t('common:buttons.cancel')}
                </Button>
                <Button onClick={handleStartSprint} disabled={!canStart}>
                  {isStarting ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Starting...
                    </>
                  ) : (
                    'Start Sprint'
                  )}
                </Button>
              </div>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const SprintPlannerDialog = defineModal<SprintPlannerDialogProps, void>(
  SprintPlannerDialogImpl
);
