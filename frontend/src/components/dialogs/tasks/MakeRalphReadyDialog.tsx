import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert } from '@/components/ui/alert';
import { specSheetsApi, makeRalphReadyApi } from '@/lib/api';
import type { Task } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { Plus, X, Loader2, CheckCircle2, Sparkles } from 'lucide-react';

export interface MakeRalphReadyDialogProps {
  taskId: string;
  taskTitle: string;
  onComplete?: () => void;
}

function ArrayField({
  label,
  items,
  onChange,
  placeholder,
}: {
  label: string;
  items: string[];
  onChange: (items: string[]) => void;
  placeholder?: string;
}) {
  const addItem = () => onChange([...items, '']);
  const removeItem = (index: number) =>
    onChange(items.filter((_, i) => i !== index));
  const updateItem = (index: number, value: string) =>
    onChange(items.map((item, i) => (i === index ? value : item)));

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label>{label}</Label>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={addItem}
          className="h-7 px-2"
        >
          <Plus className="h-3 w-3 mr-1" />
          Add
        </Button>
      </div>
      {items.map((item, index) => (
        <div key={index} className="flex gap-2">
          <Input
            value={item}
            onChange={(e) => updateItem(index, e.target.value)}
            placeholder={placeholder}
            className="flex-1"
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={() => removeItem(index)}
            className="h-9 w-9 shrink-0"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      {items.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No items yet. Click &ldquo;Add&rdquo; to add one.
        </p>
      )}
    </div>
  );
}

const MakeRalphReadyDialogImpl = NiceModal.create<MakeRalphReadyDialogProps>(
  ({ taskId, taskTitle, onComplete }) => {
    const modal = useModal();
    const [step, setStep] = useState<'spec' | 'decomposing' | 'done'>('spec');
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [children, setChildren] = useState<Task[]>([]);

    // Spec form state
    const [overview, setOverview] = useState('');
    const [requirements, setRequirements] = useState<string[]>([]);
    const [acceptanceCriteria, setAcceptanceCriteria] = useState<string[]>([]);
    const [constraints, setConstraints] = useState<string[]>([]);
    const [techNotes, setTechNotes] = useState('');
    const [isGenerating, setIsGenerating] = useState(false);
    const [generatingElapsed, setGeneratingElapsed] = useState(0);
    const abortControllerRef = useRef<AbortController | null>(null);
    const elapsedTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

    // Reset all state when dialog reopens for a different task
    useEffect(() => {
      setStep('spec');
      setError(null);
      setChildren([]);
      setOverview('');
      setRequirements([]);
      setAcceptanceCriteria([]);
      setConstraints([]);
      setTechNotes('');
      setIsGenerating(false);
      setGeneratingElapsed(0);
    }, [taskId]);

    // Clean up timer on unmount
    useEffect(() => {
      return () => {
        if (elapsedTimerRef.current) clearInterval(elapsedTimerRef.current);
        abortControllerRef.current?.abort();
      };
    }, []);

    const loadSpec = useCallback(async () => {
      try {
        setIsLoading(true);
        const spec = await specSheetsApi.get(taskId);
        if (spec) {
          setOverview(spec.overview ?? '');
          setRequirements(
            spec.requirements ? JSON.parse(spec.requirements) : []
          );
          setAcceptanceCriteria(
            spec.acceptance_criteria ? JSON.parse(spec.acceptance_criteria) : []
          );
          setConstraints(spec.constraints ? JSON.parse(spec.constraints) : []);
          setTechNotes(spec.tech_notes ?? '');
        }
      } catch {
        // No spec exists yet
      } finally {
        setIsLoading(false);
      }
    }, [taskId]);

    useEffect(() => {
      if (modal.visible) {
        loadSpec();
      }
    }, [modal.visible, loadSpec]);

    const handleGenerateSpec = async () => {
      setError(null);
      setIsGenerating(true);
      setGeneratingElapsed(0);

      // Start elapsed timer
      const start = Date.now();
      elapsedTimerRef.current = setInterval(() => {
        setGeneratingElapsed(Math.floor((Date.now() - start) / 1000));
      }, 1000);

      // Set up abort controller
      const controller = new AbortController();
      abortControllerRef.current = controller;

      try {
        const result = await specSheetsApi.generateSpec(taskId);
        if (controller.signal.aborted) return;
        setOverview(result.overview);
        setRequirements(result.requirements);
        setAcceptanceCriteria(result.acceptance_criteria);
        setConstraints(result.constraints);
        setTechNotes(result.tech_notes);
      } catch (err: unknown) {
        if (controller.signal.aborted) return;
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to generate spec';
        setError(errorMessage);
      } finally {
        setIsGenerating(false);
        if (elapsedTimerRef.current) {
          clearInterval(elapsedTimerRef.current);
          elapsedTimerRef.current = null;
        }
        abortControllerRef.current = null;
      }
    };

    const handleCancelGenerate = () => {
      abortControllerRef.current?.abort();
      setIsGenerating(false);
      setGeneratingElapsed(0);
      if (elapsedTimerRef.current) {
        clearInterval(elapsedTimerRef.current);
        elapsedTimerRef.current = null;
      }
    };

    const handleNext = async () => {
      setError(null);
      setStep('decomposing');

      try {
        const result = await makeRalphReadyApi.makeReady(taskId, {
          overview: overview || null,
          requirements:
            requirements.length > 0
              ? JSON.stringify(requirements.filter((r) => r.trim()))
              : null,
          acceptance_criteria:
            acceptanceCriteria.length > 0
              ? JSON.stringify(acceptanceCriteria.filter((r) => r.trim()))
              : null,
          constraints:
            constraints.length > 0
              ? JSON.stringify(constraints.filter((r) => r.trim()))
              : null,
          tech_notes: techNotes || null,
        });

        setChildren(result);
        setStep('done');
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to decompose task';
        setError(errorMessage);
        setStep('spec');
      }
    };

    const handleClose = () => {
      onComplete?.();
      modal.resolve();
      modal.hide();
    };

    const handleCancel = () => {
      modal.reject();
      modal.hide();
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => !open && handleCancel()}
      >
        <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Make Ralph Ready</DialogTitle>
            <DialogDescription>{taskTitle}</DialogDescription>
          </DialogHeader>

          {step === 'spec' && (
            <>
              {isLoading ? (
                <div className="py-8 text-center text-muted-foreground">
                  Loading...
                </div>
              ) : (
                <div className="space-y-6">
                  <div className="flex items-center justify-between">
                    <p className="text-sm text-muted-foreground">
                      Step 1: Define the spec sheet, then decompose into
                      stories.
                    </p>
                    {!isGenerating && (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={handleGenerateSpec}
                      >
                        <Sparkles className="h-4 w-4 mr-1.5" />
                        Generate with AI
                      </Button>
                    )}
                  </div>

                  {isGenerating && (
                    <div className="flex items-center justify-between rounded-md border border-border bg-muted/50 px-4 py-3">
                      <div className="flex items-center gap-3">
                        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                        <div>
                          <p className="text-sm font-medium">
                            Analyzing codebase and generating spec...
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {generatingElapsed < 10
                              ? 'This usually takes 30\u201360 seconds.'
                              : `${generatingElapsed}s elapsed`}
                          </p>
                        </div>
                      </div>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={handleCancelGenerate}
                      >
                        Cancel
                      </Button>
                    </div>
                  )}

                  <div className="space-y-2">
                    <Label htmlFor="overview">Overview</Label>
                    <Textarea
                      id="overview"
                      value={overview}
                      onChange={(e) => setOverview(e.target.value)}
                      placeholder="Brief overview of what this task should accomplish..."
                      rows={3}
                    />
                  </div>

                  <ArrayField
                    label="Requirements"
                    items={requirements}
                    onChange={setRequirements}
                    placeholder="Enter a requirement..."
                  />

                  <ArrayField
                    label="Acceptance Criteria"
                    items={acceptanceCriteria}
                    onChange={setAcceptanceCriteria}
                    placeholder="Enter an acceptance criterion..."
                  />

                  <ArrayField
                    label="Constraints"
                    items={constraints}
                    onChange={setConstraints}
                    placeholder="Enter a constraint..."
                  />

                  <div className="space-y-2">
                    <Label htmlFor="techNotes">Technical Notes</Label>
                    <Textarea
                      id="techNotes"
                      value={techNotes}
                      onChange={(e) => setTechNotes(e.target.value)}
                      placeholder="Technical notes, implementation hints..."
                      rows={3}
                    />
                  </div>
                </div>
              )}

              {error && (
                <Alert variant="destructive" className="mt-4">
                  {error}
                </Alert>
              )}

              <DialogFooter>
                <Button variant="outline" onClick={handleCancel}>
                  Cancel
                </Button>
                <Button
                  onClick={handleNext}
                  disabled={isLoading || isGenerating}
                >
                  Save Spec & Decompose
                </Button>
              </DialogFooter>
            </>
          )}

          {step === 'decomposing' && (
            <div className="py-12 text-center space-y-4">
              <Loader2 className="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
              <p className="text-muted-foreground">
                Saving spec and decomposing into stories...
              </p>
              <p className="text-xs text-muted-foreground">
                This may take a minute.
              </p>
            </div>
          )}

          {step === 'done' && (
            <>
              <div className="space-y-4">
                <div className="flex items-center gap-2 text-green-600">
                  <CheckCircle2 className="h-5 w-5" />
                  <span className="font-medium">
                    Decomposed into {children.length} stories
                  </span>
                </div>

                <div className="space-y-2 max-h-60 overflow-y-auto">
                  {children.map((child, i) => (
                    <div
                      key={child.id}
                      className="flex items-start gap-2 p-2 rounded bg-muted/50"
                    >
                      <span className="text-xs text-muted-foreground font-mono mt-0.5">
                        {i + 1}.
                      </span>
                      <div>
                        <p className="text-sm font-medium">{child.title}</p>
                        {child.description && (
                          <p className="text-xs text-muted-foreground line-clamp-2">
                            {child.description}
                          </p>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              <DialogFooter>
                <Button onClick={handleClose}>Done</Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    );
  }
);

export const MakeRalphReadyDialog = defineModal<
  MakeRalphReadyDialogProps,
  void
>(MakeRalphReadyDialogImpl);
