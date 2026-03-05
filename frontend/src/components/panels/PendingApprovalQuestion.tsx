import { useState, useEffect, useCallback } from 'react';
import { approvalsApi } from '@/lib/api';
import type { PendingApprovalRecord } from '@/lib/api';
import { MessageCircleQuestion } from 'lucide-react';
import { Button } from '../ui/button';

interface Question {
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  multiSelect?: boolean;
}

interface PendingApprovalQuestionProps {
  taskId: string;
  executionProcessId?: string;
}

export function PendingApprovalQuestion({
  taskId,
}: PendingApprovalQuestionProps) {
  const [approval, setApproval] = useState<PendingApprovalRecord | null>(null);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const fetchPending = async () => {
      try {
        const approvals = await approvalsApi.getPending(taskId);
        if (!cancelled && approvals.length > 0) {
          const record = approvals[0];
          setApproval(record);
          try {
            const toolInput = JSON.parse(record.tool_input);
            if (toolInput.questions) {
              setQuestions(toolInput.questions);
            }
          } catch {
            // Invalid JSON in tool_input
          }
        }
      } catch {
        // Ignore fetch errors
      }
    };
    fetchPending();
    // Poll every 5s while no approval found
    const interval = setInterval(() => {
      if (!approval) fetchPending();
    }, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [taskId, approval]);

  const handleSelect = useCallback((question: string, value: string) => {
    setAnswers((prev) => ({ ...prev, [question]: value }));
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!approval) return;
    setSubmitting(true);
    try {
      await approvalsApi.respond(approval.id, {
        execution_process_id: approval.execution_process_id,
        status: { status: 'approved' },
        updated_input: { answers },
      });
      setSubmitted(true);
    } catch (err) {
      console.error('Failed to submit approval response:', err);
    } finally {
      setSubmitting(false);
    }
  }, [approval, answers]);

  if (submitted) {
    return (
      <div className="py-6 flex flex-col items-center gap-2 text-muted-foreground">
        <p className="text-sm">
          Answers submitted. Re-generating plan with your input...
        </p>
      </div>
    );
  }

  if (!approval || questions.length === 0) {
    return null;
  }

  const allAnswered = questions.every((q) => answers[q.question]);

  return (
    <div className="border rounded-lg p-4 bg-muted/30 space-y-4">
      <div className="flex items-center gap-2 text-sm font-medium">
        <MessageCircleQuestion className="h-4 w-4 text-purple-500" />
        <span>Planning needs your input to continue</span>
      </div>

      <p className="text-xs text-muted-foreground">
        This question was asked during plan generation. Answer to continue
        planning.
      </p>

      {questions.map((q) => (
        <div key={q.question} className="space-y-2">
          <p className="text-sm font-medium">{q.question}</p>
          <div className="flex flex-wrap gap-2">
            {q.options.map((opt) => (
              <button
                key={opt.label}
                onClick={() => handleSelect(q.question, opt.label)}
                className={`px-3 py-1.5 text-sm rounded-md border transition-colors ${
                  answers[q.question] === opt.label
                    ? 'bg-primary text-primary-foreground border-primary'
                    : 'bg-background hover:bg-muted border-border'
                }`}
                title={opt.description}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>
      ))}

      <Button
        onClick={handleSubmit}
        disabled={!allAnswered || submitting}
        size="sm"
      >
        {submitting ? 'Submitting...' : 'Continue Planning'}
      </Button>
    </div>
  );
}
