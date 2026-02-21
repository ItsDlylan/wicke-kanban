import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { planningSessionsApi } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Plus, Pencil, Trash2, X, Check } from 'lucide-react';
import type { PlanningSession } from 'shared/types';

interface PlanningSessionsPanelProps {
  taskId: string;
}

export function PlanningSessionsPanel({ taskId }: PlanningSessionsPanelProps) {
  const queryClient = useQueryClient();
  const [isAdding, setIsAdding] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [sessionRef, setSessionRef] = useState('');
  const [notes, setNotes] = useState('');

  const { data: sessions = [], isLoading } = useQuery({
    queryKey: ['planning-sessions', taskId],
    queryFn: () => planningSessionsApi.list(taskId),
  });

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: ['planning-sessions', taskId],
    });
  }, [queryClient, taskId]);

  const createMutation = useMutation({
    mutationFn: () =>
      planningSessionsApi.create(taskId, {
        session_ref: sessionRef,
        notes: notes || null,
      }),
    onSuccess: () => {
      invalidate();
      setIsAdding(false);
      setSessionRef('');
      setNotes('');
    },
  });

  const updateMutation = useMutation({
    mutationFn: (session: PlanningSession) =>
      planningSessionsApi.update(taskId, session.id, {
        session_ref: sessionRef || null,
        notes: notes || null,
      }),
    onSuccess: () => {
      invalidate();
      setEditingId(null);
      setSessionRef('');
      setNotes('');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (sessionId: string) =>
      planningSessionsApi.delete(taskId, sessionId),
    onSuccess: invalidate,
  });

  const startEdit = (session: PlanningSession) => {
    setEditingId(session.id);
    setSessionRef(session.session_ref);
    setNotes(session.notes ?? '');
  };

  const cancelEdit = () => {
    setEditingId(null);
    setIsAdding(false);
    setSessionRef('');
    setNotes('');
  };

  if (isLoading) {
    return (
      <div className="text-sm text-muted-foreground">Loading sessions...</div>
    );
  }

  return (
    <div className="border rounded-md p-3 space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Planning Sessions</span>
        {!isAdding && (
          <Button variant="icon" onClick={() => setIsAdding(true)}>
            <Plus size={16} />
          </Button>
        )}
      </div>

      {isAdding && (
        <div className="space-y-2 border rounded p-2">
          <Input
            placeholder="Session reference (e.g. claude-code-abc123)"
            value={sessionRef}
            onChange={(e) => setSessionRef(e.target.value)}
            autoFocus
          />
          <Input
            placeholder="Notes (optional)"
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
          />
          <div className="flex gap-1 justify-end">
            <Button variant="icon" onClick={cancelEdit}>
              <X size={14} />
            </Button>
            <Button
              variant="icon"
              onClick={() => createMutation.mutate()}
              disabled={!sessionRef.trim() || createMutation.isPending}
            >
              <Check size={14} />
            </Button>
          </div>
        </div>
      )}

      {sessions.length === 0 && !isAdding && (
        <p className="text-xs text-muted-foreground">
          No planning sessions linked yet.
        </p>
      )}

      {sessions.map((session) =>
        editingId === session.id ? (
          <div key={session.id} className="space-y-2 border rounded p-2">
            <Input
              placeholder="Session reference"
              value={sessionRef}
              onChange={(e) => setSessionRef(e.target.value)}
              autoFocus
            />
            <Input
              placeholder="Notes (optional)"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
            <div className="flex gap-1 justify-end">
              <Button variant="icon" onClick={cancelEdit}>
                <X size={14} />
              </Button>
              <Button
                variant="icon"
                onClick={() => updateMutation.mutate(session)}
                disabled={!sessionRef.trim() || updateMutation.isPending}
              >
                <Check size={14} />
              </Button>
            </div>
          </div>
        ) : (
          <div
            key={session.id}
            className="flex items-start justify-between gap-2 text-sm border rounded p-2"
          >
            <div className="min-w-0 flex-1">
              <code className="text-xs bg-muted px-1.5 py-0.5 rounded break-all">
                {session.session_ref}
              </code>
              {session.notes && (
                <p className="text-xs text-muted-foreground mt-1">
                  {session.notes}
                </p>
              )}
            </div>
            <div className="flex gap-0.5 shrink-0">
              <Button variant="icon" onClick={() => startEdit(session)}>
                <Pencil size={12} />
              </Button>
              <Button
                variant="icon"
                onClick={() => deleteMutation.mutate(session.id)}
                disabled={deleteMutation.isPending}
              >
                <Trash2 size={12} />
              </Button>
            </div>
          </div>
        )
      )}
    </div>
  );
}
