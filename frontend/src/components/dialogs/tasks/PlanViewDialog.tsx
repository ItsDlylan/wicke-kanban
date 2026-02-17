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
import { Loader2 } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

export interface PlanViewDialogProps {
  taskId: string;
  taskTitle: string;
  plan: string | null;
  planStatus: string | null;
}

const PlanViewDialogImpl = NiceModal.create<PlanViewDialogProps>(
  ({ taskId, taskTitle, plan, planStatus }) => {
    const modal = useModal();
    const [currentPlan, setCurrentPlan] = useState(plan);
    const [currentStatus, setCurrentStatus] = useState(planStatus);
    const [isRegenerating, setIsRegenerating] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Poll for updates while generating
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

    const handleClose = () => {
      modal.reject();
      modal.hide();
    };

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
);

export const PlanViewDialog = defineModal<PlanViewDialogProps, void>(
  PlanViewDialogImpl
);
