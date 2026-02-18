import { useState, useEffect, useMemo, useCallback } from 'react';
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
import { ralphSessionApi } from '@/lib/api';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { useQueryClient } from '@tanstack/react-query';
import type { ExecutorProfileId, TaskWithAttemptStatus } from 'shared/types';
import { Loader2, ChevronUp, ChevronDown, X } from 'lucide-react';

export interface RalphSessionDialogProps {
  taskId: string;
  taskTitle: string;
}

const RalphSessionDialogImpl = NiceModal.create<RalphSessionDialogProps>(
  ({ taskId, taskTitle }) => {
    const modal = useModal();
    const { projectId } = useProject();
    const { profiles, config } = useUserSystem();
    const queryClient = useQueryClient();

    const { tasks } = useProjectTasks(projectId || '');

    const [selectedIds, setSelectedIds] = useState<string[]>([]);
    const [isStarting, setIsStarting] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [selectedProfile, setSelectedProfile] =
      useState<ExecutorProfileId | null>(null);

    const { data: projectRepos = [], isLoading: isLoadingRepos } =
      useProjectRepos(projectId || '', { enabled: modal.visible });

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

    // Ralph-ready tasks: in Ready status with spec + children
    const ralphReadyTasks = useMemo(() => {
      return tasks.filter(
        (t) => t.status === 'ready' && t.has_spec && t.has_children
      );
    }, [tasks]);

    // Pre-select the current task if it's ralph-ready
    useEffect(() => {
      if (!modal.visible) {
        setSelectedIds([]);
        setError(null);
        setIsStarting(false);
        return;
      }
      const isCurrentRalphReady = ralphReadyTasks.some((t) => t.id === taskId);
      if (isCurrentRalphReady) {
        setSelectedIds([taskId]);
      }
    }, [modal.visible, taskId, ralphReadyTasks]);

    const handleToggle = useCallback((id: string) => {
      setSelectedIds((prev) => {
        if (prev.includes(id)) {
          return prev.filter((x) => x !== id);
        }
        return [...prev, id];
      });
    }, []);

    const handleRemove = useCallback((id: string) => {
      setSelectedIds((prev) => prev.filter((x) => x !== id));
    }, []);

    const handleMoveUp = useCallback((index: number) => {
      if (index <= 0) return;
      setSelectedIds((prev) => {
        const next = [...prev];
        [next[index - 1], next[index]] = [next[index], next[index - 1]];
        return next;
      });
    }, []);

    const handleMoveDown = useCallback((index: number) => {
      setSelectedIds((prev) => {
        if (index >= prev.length - 1) return prev;
        const next = [...prev];
        [next[index], next[index + 1]] = [next[index + 1], next[index]];
        return next;
      });
    }, []);

    // Build a lookup for selected tasks
    const selectedTasks = useMemo(() => {
      const byId = new Map<string, TaskWithAttemptStatus>();
      ralphReadyTasks.forEach((t) => byId.set(t.id, t));
      return selectedIds
        .map((id) => byId.get(id))
        .filter(Boolean) as TaskWithAttemptStatus[];
    }, [selectedIds, ralphReadyTasks]);

    const allBranchesSelected = repoBranchConfigs.every(
      (c) => c.targetBranch !== null
    );

    const canStart = Boolean(
      selectedIds.length > 0 &&
        effectiveProfile &&
        allBranchesSelected &&
        projectRepos.length > 0 &&
        !isStarting
    );

    const handleStartSession = async () => {
      if (!canStart) return;
      setIsStarting(true);
      setError(null);
      try {
        const repos = getWorkspaceRepoInputs();
        await ralphSessionApi.create({
          task_ids: selectedIds,
          repos,
          executor_profile_id: effectiveProfile,
        });
        queryClient.invalidateQueries({ queryKey: ['tasks'] });
        modal.resolve();
        modal.hide();
      } catch (err) {
        setError(
          err instanceof Error ? err.message : 'Failed to start Ralph session'
        );
      } finally {
        setIsStarting(false);
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) modal.hide();
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent
          className="max-w-4xl max-h-[85vh] flex flex-col"
          onKeyDownCapture={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              modal.hide();
            }
          }}
        >
          <DialogHeader>
            <DialogTitle>Ralph Session: {taskTitle}</DialogTitle>
          </DialogHeader>

          <div className="flex-1 overflow-hidden min-h-0 flex gap-4">
            {/* Left panel: Ralph-ready tasks */}
            <div className="flex-1 flex flex-col min-w-0">
              <h3 className="text-sm font-medium mb-2">
                Ralph-Ready Tasks ({ralphReadyTasks.length})
              </h3>
              <div className="flex-1 overflow-y-auto border rounded-md divide-y">
                {ralphReadyTasks.length === 0 ? (
                  <div className="p-4 text-center text-sm text-muted-foreground">
                    No ralph-ready tasks found. Use &ldquo;Make Ralph
                    Ready&rdquo; to prepare tasks.
                  </div>
                ) : (
                  ralphReadyTasks.map((task) => (
                    <label
                      key={task.id}
                      className="flex items-start gap-3 p-3 hover:bg-muted/50 cursor-pointer"
                    >
                      <Checkbox
                        checked={selectedIds.includes(task.id)}
                        onCheckedChange={() => handleToggle(task.id)}
                        className="mt-0.5"
                      />
                      <div className="flex-1 min-w-0">
                        <span className="text-sm font-medium truncate block">
                          {task.title}
                        </span>
                        {task.description && (
                          <span className="text-xs text-muted-foreground line-clamp-1">
                            {task.description}
                          </span>
                        )}
                      </div>
                    </label>
                  ))
                )}
              </div>
            </div>

            {/* Right panel: Selected tasks (ordered) + config */}
            <div className="flex-1 flex flex-col min-w-0">
              <h3 className="text-sm font-medium mb-2">
                Execution Order ({selectedTasks.length} selected)
              </h3>
              <div className="flex-1 overflow-y-auto border rounded-md divide-y mb-3">
                {selectedTasks.length === 0 ? (
                  <div className="p-4 text-center text-sm text-muted-foreground">
                    Select tasks from the left panel.
                  </div>
                ) : (
                  selectedTasks.map((task, i) => (
                    <div key={task.id} className="flex items-center gap-2 p-2">
                      <span className="text-xs text-muted-foreground font-mono w-5 text-right shrink-0">
                        {i + 1}.
                      </span>
                      <span className="text-sm font-medium truncate flex-1">
                        {task.title}
                      </span>
                      <div className="flex items-center gap-0.5 shrink-0">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          disabled={i === 0}
                          onClick={() => handleMoveUp(i)}
                        >
                          <ChevronUp className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          disabled={i === selectedTasks.length - 1}
                          onClick={() => handleMoveDown(i)}
                        >
                          <ChevronDown className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          onClick={() => handleRemove(task.id)}
                        >
                          <X className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                  ))
                )}
              </div>

              {/* Configuration */}
              <div className="space-y-3 border rounded-md p-3">
                <h3 className="text-sm font-medium">Configuration</h3>
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
            </div>
          </div>

          {error && <div className="text-sm text-destructive">{error}</div>}

          <DialogFooter>
            <div className="flex items-center justify-between w-full">
              <span className="text-sm text-muted-foreground">
                {selectedIds.length > 0 &&
                  `${selectedIds.length} task${selectedIds.length > 1 ? 's' : ''} selected`}
              </span>
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => modal.hide()}>
                  Cancel
                </Button>
                <Button onClick={handleStartSession} disabled={!canStart}>
                  {isStarting ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Starting...
                    </>
                  ) : (
                    'Start Ralph Session'
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

export const RalphSessionDialog = defineModal<RalphSessionDialogProps, void>(
  RalphSessionDialogImpl
);
