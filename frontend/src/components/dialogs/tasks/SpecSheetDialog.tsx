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
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert } from '@/components/ui/alert';
import { specSheetsApi } from '@/lib/api';
import type { CreateSpecSheet } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { Plus, X } from 'lucide-react';

export interface SpecSheetDialogProps {
  taskId: string;
  taskTitle: string;
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
          No items yet. Click "Add" to add one.
        </p>
      )}
    </div>
  );
}

const SpecSheetDialogImpl = NiceModal.create<SpecSheetDialogProps>(
  ({ taskId, taskTitle }) => {
    const modal = useModal();
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const [overview, setOverview] = useState('');
    const [requirements, setRequirements] = useState<string[]>([]);
    const [acceptanceCriteria, setAcceptanceCriteria] = useState<string[]>([]);
    const [constraints, setConstraints] = useState<string[]>([]);
    const [techNotes, setTechNotes] = useState('');

    // Reset form state when dialog reopens for a different task
    useEffect(() => {
      setOverview('');
      setRequirements([]);
      setAcceptanceCriteria([]);
      setConstraints([]);
      setTechNotes('');
      setError(null);
    }, [taskId]);

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
        // No spec exists yet, that's fine
      } finally {
        setIsLoading(false);
      }
    }, [taskId]);

    useEffect(() => {
      if (modal.visible) {
        loadSpec();
      }
    }, [modal.visible, loadSpec]);

    const handleSave = async () => {
      setIsSaving(true);
      setError(null);

      try {
        const data: CreateSpecSheet = {
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
        };

        await specSheetsApi.createOrUpdate(taskId, data);
        modal.resolve();
        modal.hide();
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to save spec sheet';
        setError(errorMessage);
      } finally {
        setIsSaving(false);
      }
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
            <DialogTitle>Spec Sheet</DialogTitle>
            <DialogDescription>{taskTitle}</DialogDescription>
          </DialogHeader>

          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground">
              Loading spec sheet...
            </div>
          ) : (
            <div className="space-y-6">
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
                  placeholder="Technical notes, implementation hints, related files..."
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
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isSaving}
            >
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={isSaving || isLoading}>
              {isSaving ? 'Saving...' : 'Save Spec'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const SpecSheetDialog = defineModal<SpecSheetDialogProps, void>(
  SpecSheetDialogImpl
);
