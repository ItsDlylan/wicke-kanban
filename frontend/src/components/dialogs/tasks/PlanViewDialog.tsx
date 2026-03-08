import { useCallback, useEffect, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Alert } from '@/components/ui/alert';
import { tasksApi } from '@/lib/api';
import { Loader2, Pencil, ChevronDown, ChevronRight } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { BaseCodingAgent } from 'shared/types';
import { usePlanEditSession } from '@/hooks/usePlanEditSession';
import { usePlanWorkspace } from '@/hooks/usePlanWorkspace';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import { EntriesProvider } from '@/contexts/EntriesContext';
import { ConversationList } from '@/components/ui-new/containers/ConversationListContainer';
import { PlanEditChatInput } from './PlanEditChatInput';
import { cn } from '@/lib/utils';

export interface PlanViewDialogProps {
  taskId: string;
  taskTitle: string;
  taskDescription?: string;
  plan: string | null;
  planStatus: string | null;
}

const PlanViewDialogImpl = NiceModal.create<PlanViewDialogProps>(
  ({ taskId, taskTitle, taskDescription, plan, planStatus }) => {
    const modal = useModal();
    const [currentPlan, setCurrentPlan] = useState(plan);
    const [currentStatus, setCurrentStatus] = useState(planStatus);
    const [isRegenerating, setIsRegenerating] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Edit mode state
    const [isEditMode, setIsEditMode] = useState(false);
    const [isPlanCollapsed, setIsPlanCollapsed] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [saveDraftText, setSaveDraftText] = useState('');
    const [showSaveConfirm, setShowSaveConfirm] = useState(false);

    const planEditSession = usePlanEditSession({
      taskId,
      taskTitle,
      taskDescription,
      currentPlan,
      executor: BaseCodingAgent.CLAUDE_CODE,
    });

    // Auto-plan streaming workspace
    const planWorkspace = usePlanWorkspace(taskId, currentStatus);

    // Sync state when dialog reopens with a different task
    useEffect(() => {
      setCurrentPlan(plan);
      setCurrentStatus(planStatus);
      setError(null);
      setIsEditMode(false);
      setShowSaveConfirm(false);
    }, [taskId, plan, planStatus]);

    // Poll for plan completion while generating
    useEffect(() => {
      if (currentStatus !== 'generating') return;

      const interval = setInterval(async () => {
        try {
          const task = await tasksApi.getById(taskId);
          setCurrentPlan(task.plan);
          setCurrentStatus(task.plan_status);
        } catch {
          // Ignore polling errors
        }
      }, 3000);

      return () => clearInterval(interval);
    }, [taskId, currentStatus]);

    const handleRegenerate = useCallback(async () => {
      setIsRegenerating(true);
      setError(null);

      try {
        await tasksApi.regeneratePlan(taskId);
        setCurrentStatus('generating');
        setCurrentPlan(null);
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to regenerate plan';
        setError(errorMessage);
      } finally {
        setIsRegenerating(false);
      }
    }, [taskId]);

    const handleEnterEditMode = useCallback(async () => {
      setIsEditMode(true);
      setError(null);
      await planEditSession.startSession();
    }, [planEditSession]);

    const handleCancelEditMode = useCallback(async () => {
      setIsEditMode(false);
      setShowSaveConfirm(false);
      await planEditSession.cleanup();
    }, [planEditSession]);

    const handleStartSave = useCallback(() => {
      // Pre-fill with current plan text
      setSaveDraftText(currentPlan ?? '');
      setShowSaveConfirm(true);
    }, [currentPlan]);

    const handleConfirmSave = useCallback(async () => {
      setIsSaving(true);
      try {
        await planEditSession.savePlan(saveDraftText);
        setCurrentPlan(saveDraftText);
        setCurrentStatus('completed');
        setIsEditMode(false);
        setShowSaveConfirm(false);
        await planEditSession.cleanup();
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to save plan';
        setError(errorMessage);
      } finally {
        setIsSaving(false);
      }
    }, [saveDraftText, planEditSession]);

    const handleClose = useCallback(() => {
      if (isEditMode) {
        planEditSession.cleanup();
      }
      modal.reject();
      modal.hide();
    }, [isEditMode, planEditSession, modal]);

    // --- Generating Mode (streaming) ---
    const isGeneratingWithStream =
      currentStatus === 'generating' && planWorkspace?.session;

    if (!isEditMode && isGeneratingWithStream) {
      return (
        <Dialog
          open={modal.visible}
          onOpenChange={(open) => !open && handleClose()}
        >
          <DialogContent className="max-w-5xl max-h-[90vh] flex flex-col p-0 gap-0">
            <DialogHeader className="px-6 pt-6 pb-3">
              <DialogTitle className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" />
                Generating Plan
              </DialogTitle>
              <DialogDescription>{taskTitle}</DialogDescription>
            </DialogHeader>

            <ExecutionProcessesProvider
              attemptId={planWorkspace.id}
              sessionId={planWorkspace.session!.id}
            >
              <EntriesProvider key={planWorkspace.id}>
                <div className="flex-1 min-h-0 overflow-hidden">
                  <ConversationList attempt={planWorkspace} />
                </div>
              </EntriesProvider>
            </ExecutionProcessesProvider>

            <DialogFooter className="px-6 py-3 border-t">
              <Button variant="outline" onClick={handleClose}>
                Close
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }

    // --- View Mode ---
    if (!isEditMode) {
      return (
        <Dialog
          open={modal.visible}
          onOpenChange={(open) => !open && handleClose()}
        >
          <DialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>Implementation Plan</DialogTitle>
              <DialogDescription>{taskTitle}</DialogDescription>
            </DialogHeader>

            {currentStatus === 'generating' && (
              <div className="py-8 flex flex-col items-center gap-3 text-muted-foreground">
                <Loader2 className="h-6 w-6 animate-spin" />
                <p>Generating plan...</p>
              </div>
            )}

            {currentStatus === 'failed' && (
              <Alert variant="destructive" className="mt-2">
                Plan generation failed.
                {currentPlan && <p className="mt-1 text-sm">{currentPlan}</p>}
              </Alert>
            )}

            {currentStatus === 'completed' && currentPlan && (
              <div className="whitespace-pre-wrap text-sm font-mono bg-muted p-4 rounded-md max-h-[50vh] overflow-y-auto">
                {currentPlan}
              </div>
            )}

            {currentStatus === 'pending' && (
              <div className="py-8 text-center text-muted-foreground">
                Plan generation is pending...
              </div>
            )}

            {!currentStatus && (
              <div className="py-8 text-center text-muted-foreground">
                No plan has been generated for this task.
              </div>
            )}

            {error && (
              <Alert variant="destructive" className="mt-4">
                {error}
              </Alert>
            )}

            <DialogFooter>
              <Button variant="outline" onClick={handleClose}>
                Close
              </Button>
              <Button
                variant="outline"
                onClick={handleEnterEditMode}
                disabled={currentStatus === 'generating'}
              >
                <Pencil className="h-4 w-4 mr-2" />
                Edit Plan
              </Button>
              <Button
                onClick={handleRegenerate}
                disabled={isRegenerating || currentStatus === 'generating'}
              >
                {isRegenerating ? 'Starting...' : 'Regenerate Plan'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }

    // --- Edit Mode ---
    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => !open && handleClose()}
      >
        <DialogContent className="max-w-5xl max-h-[90vh] flex flex-col p-0 gap-0">
          <DialogHeader className="px-6 pt-6 pb-3">
            <DialogTitle>Edit Implementation Plan</DialogTitle>
            <DialogDescription>{taskTitle}</DialogDescription>
          </DialogHeader>

          {/* Collapsible current plan reference */}
          {currentPlan && (
            <div className="border-b px-6 pb-3">
              <button
                onClick={() => setIsPlanCollapsed(!isPlanCollapsed)}
                className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                {isPlanCollapsed ? (
                  <ChevronRight className="h-4 w-4" />
                ) : (
                  <ChevronDown className="h-4 w-4" />
                )}
                Current Plan
              </button>
              {!isPlanCollapsed && (
                <div className="mt-2 whitespace-pre-wrap text-xs font-mono bg-muted p-3 rounded-md max-h-[20vh] overflow-y-auto">
                  {currentPlan}
                </div>
              )}
            </div>
          )}

          {/* Session initialization state */}
          {planEditSession.isInitializing && (
            <div className="flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground py-12">
              <Loader2 className="h-6 w-6 animate-spin" />
              <p>Starting plan editing session...</p>
            </div>
          )}

          {planEditSession.error && (
            <div className="px-6 py-4">
              <Alert variant="destructive">{planEditSession.error}</Alert>
            </div>
          )}

          {/* Conversation area */}
          {planEditSession.workspaceWithSession && planEditSession.session && (
            <ExecutionProcessesProvider
              attemptId={planEditSession.workspace?.id}
              sessionId={planEditSession.session.id}
            >
              <EntriesProvider key={planEditSession.workspace?.id}>
                <div className="flex-1 min-h-0 overflow-hidden">
                  <ConversationList
                    attempt={planEditSession.workspaceWithSession}
                  />
                </div>
                <PlanEditChatInput
                  sessionId={planEditSession.session.id}
                  executor={BaseCodingAgent.CLAUDE_CODE}
                />
              </EntriesProvider>
            </ExecutionProcessesProvider>
          )}

          {/* Save confirmation overlay */}
          {showSaveConfirm && (
            <div className="border-t px-6 py-4 space-y-3">
              <p className="text-sm font-medium">
                Paste or edit the final plan text below, then confirm:
              </p>
              <textarea
                value={saveDraftText}
                onChange={(e) => setSaveDraftText(e.target.value)}
                rows={8}
                className="w-full resize-y rounded-md border bg-background px-3 py-2 text-sm font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
              <div className="flex justify-end gap-2">
                <Button
                  variant="outline"
                  onClick={() => setShowSaveConfirm(false)}
                  disabled={isSaving}
                >
                  Back
                </Button>
                <Button onClick={handleConfirmSave} disabled={isSaving}>
                  {isSaving ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Saving...
                    </>
                  ) : (
                    'Confirm Save'
                  )}
                </Button>
              </div>
            </div>
          )}

          {/* Footer */}
          {!showSaveConfirm && (
            <DialogFooter
              className={cn(
                'px-6 py-3 border-t',
                planEditSession.isInitializing && 'hidden'
              )}
            >
              <Button variant="outline" onClick={handleCancelEditMode}>
                Cancel
              </Button>
              <Button
                onClick={handleStartSave}
                disabled={!planEditSession.session}
              >
                Save Plan
              </Button>
            </DialogFooter>
          )}

          {error && (
            <div className="px-6 pb-4">
              <Alert variant="destructive">{error}</Alert>
            </div>
          )}
        </DialogContent>
      </Dialog>
    );
  }
);

export const PlanViewDialog = defineModal<PlanViewDialogProps, void>(
  PlanViewDialogImpl
);
