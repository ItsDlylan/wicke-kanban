import {
  useCallback,
  useState,
  useMemo,
  useEffect,
  useRef,
  useContext,
} from 'react';
import type { ReactNode } from 'react';
import type { ToolStatus, JsonValue } from 'shared/types';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { approvalsApi } from '@/lib/api';
import { Check, MessageCircleQuestion } from 'lucide-react';
import { cn } from '@/lib/utils';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { useHotkeysContext } from 'react-hotkeys-hook';
import { TabNavContext } from '@/contexts/TabNavigationContext';
import { Scope } from '@/keyboard';

// ---------- Types ----------

interface QuestionOption {
  label: string;
  description?: string;
  markdown?: string;
  mermaid?: string;
  table?: { headers: string[]; rows: string[][] };
  codeDiff?: {
    before: string;
    after: string;
    language?: string;
    fileName?: string;
  };
  stats?: {
    items: {
      label: string;
      value: string;
      trend?: 'up' | 'down' | 'neutral';
    }[];
  };
}

interface Question {
  question: string;
  header?: string;
  options: QuestionOption[];
  multiSelect?: boolean;
}

interface AskUserQuestionEntryProps {
  pendingStatus: Extract<ToolStatus, { status: 'pending_approval' }>;
  executionProcessId?: string;
  questions: unknown;
  children: ReactNode;
}

// ---------- Countdown hook (mirrors PendingApprovalEntry) ----------

function useApprovalCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date,
  paused: boolean
) {
  const totalSeconds = useMemo(() => {
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);
    return () => window.clearInterval(id);
  }, [timeoutAt, paused]);

  const percent = useMemo(
    () =>
      Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100))),
    [timeLeft, totalSeconds]
  );

  return { timeLeft, percent };
}

// ---------- Display helpers ----------

function optionToPreviewMarkdown(opt: QuestionOption): string | null {
  if (opt.mermaid?.trim()) return '```mermaid\n' + opt.mermaid.trim() + '\n```';
  if (opt.codeDiff)
    return '```code-diff\n' + JSON.stringify(opt.codeDiff, null, 2) + '\n```';
  if (opt.table)
    return '```display-table\n' + JSON.stringify(opt.table, null, 2) + '\n```';
  if (opt.stats)
    return '```stats\n' + JSON.stringify(opt.stats, null, 2) + '\n```';
  return opt.markdown?.trim() || null;
}

function optionHasPreview(opt: QuestionOption): boolean {
  return !!(
    opt.markdown?.trim() ||
    opt.mermaid?.trim() ||
    opt.table ||
    opt.codeDiff ||
    opt.stats
  );
}

// ---------- Single Question UI ----------

function QuestionCard({
  question,
  questionIndex,
  selected,
  onSelect,
  disabled,
}: {
  question: Question;
  questionIndex: number;
  selected: string[];
  onSelect: (questionIndex: number, values: string[]) => void;
  disabled: boolean;
}) {
  const [otherText, setOtherText] = useState('');
  const [showOther, setShowOther] = useState(false);
  const isMulti = question.multiSelect ?? false;

  const hasMarkdown = useMemo(
    () => !isMulti && question.options.some(optionHasPreview),
    [isMulti, question.options]
  );

  const [focusedLabel, setFocusedLabel] = useState<string | null>(() => {
    if (!isMulti) {
      const first = question.options.find(optionHasPreview);
      return first?.label ?? null;
    }
    return null;
  });

  const focusedMarkdown = useMemo(() => {
    if (!focusedLabel) return null;
    const opt = question.options.find((o) => o.label === focusedLabel);
    if (!opt) return null;
    return optionToPreviewMarkdown(opt);
  }, [focusedLabel, question.options]);

  const handleOptionClick = (label: string) => {
    if (disabled) return;
    if (label === '__other__') {
      setShowOther(true);
      return;
    }
    setShowOther(false);
    if (isMulti) {
      const next = selected.includes(label)
        ? selected.filter((s) => s !== label)
        : [...selected.filter((s) => s !== '__other__'), label];
      onSelect(questionIndex, next);
    } else {
      onSelect(questionIndex, [label]);
    }
  };

  const handleOtherSubmit = () => {
    if (disabled || !otherText.trim()) return;
    onSelect(questionIndex, [otherText.trim()]);
    setShowOther(false);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        {question.header && (
          <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide bg-muted px-1.5 py-0.5 rounded">
            {question.header}
          </span>
        )}
        <span className="text-sm font-medium">{question.question}</span>
      </div>

      {hasMarkdown ? (
        <div className="flex gap-3">
          {/* Left column — vertical option list */}
          <div className="w-48 shrink-0 flex flex-col gap-1">
            {question.options.map((opt) => {
              const isSelected = selected.includes(opt.label);
              const isFocused = focusedLabel === opt.label;
              return (
                <button
                  key={opt.label}
                  className={cn(
                    'w-full text-left rounded-md border px-3 py-2 text-sm transition-colors',
                    isSelected
                      ? 'border-primary bg-primary text-primary-foreground'
                      : isFocused
                        ? 'border-ring bg-accent'
                        : 'border-border hover:bg-accent',
                    disabled && 'opacity-50 cursor-not-allowed'
                  )}
                  onClick={() => handleOptionClick(opt.label)}
                  onMouseEnter={() => setFocusedLabel(opt.label)}
                  disabled={disabled}
                >
                  <span className="flex items-center gap-1.5">
                    {isSelected && <Check className="h-3 w-3 shrink-0" />}
                    <span className="font-medium">{opt.label}</span>
                  </span>
                  {opt.description && (
                    <span className="block text-xs text-muted-foreground mt-0.5">
                      {opt.description}
                    </span>
                  )}
                </button>
              );
            })}
            {/* "Other" option */}
            <button
              className={cn(
                'w-full text-left rounded-md border px-3 py-2 text-sm transition-colors',
                showOther
                  ? 'border-ring bg-accent'
                  : 'border-border hover:bg-accent',
                disabled && 'opacity-50 cursor-not-allowed'
              )}
              onClick={() => handleOptionClick('__other__')}
              onMouseEnter={() => setFocusedLabel(null)}
              disabled={disabled}
            >
              <span className="font-medium">Other</span>
            </button>
          </div>

          {/* Right column — markdown preview */}
          <div className="flex-1 rounded-md border border-border overflow-y-auto min-h-[200px] max-h-[400px]">
            {focusedMarkdown ? (
              <WYSIWYGEditor value={focusedMarkdown} disabled />
            ) : (
              <div className="flex items-center justify-center h-full text-sm text-muted-foreground p-4">
                Hover over an option to preview
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap gap-1.5">
          {question.options.map((opt) => {
            const isSelected = selected.includes(opt.label);
            return (
              <Tooltip key={opt.label}>
                <TooltipTrigger asChild>
                  <Button
                    variant={isSelected ? 'default' : 'outline'}
                    size="sm"
                    className="h-7 text-xs"
                    onClick={() => handleOptionClick(opt.label)}
                    disabled={disabled}
                  >
                    {isSelected && <Check className="mr-1 h-3 w-3" />}
                    {opt.label}
                  </Button>
                </TooltipTrigger>
                {opt.description && (
                  <TooltipContent>
                    <p className="max-w-xs">{opt.description}</p>
                  </TooltipContent>
                )}
              </Tooltip>
            );
          })}
          {/* "Other" option */}
          <Button
            variant={showOther ? 'default' : 'outline'}
            size="sm"
            className="h-7 text-xs"
            onClick={() => handleOptionClick('__other__')}
            disabled={disabled}
          >
            Other
          </Button>
        </div>
      )}

      {showOther && (
        <div className="flex items-center gap-2">
          <input
            type="text"
            className="flex-1 h-7 rounded border border-input bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
            placeholder="Type your answer..."
            value={otherText}
            onChange={(e) => setOtherText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleOtherSubmit();
            }}
            disabled={disabled}
            autoFocus
          />
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={handleOtherSubmit}
            disabled={disabled || !otherText.trim()}
          >
            Submit
          </Button>
        </div>
      )}
    </div>
  );
}

// ---------- Main Component ----------

const AskUserQuestionEntry = ({
  pendingStatus,
  executionProcessId,
  questions,
  children,
}: AskUserQuestionEntryProps) => {
  const [isResponding, setIsResponding] = useState(false);
  const [hasResponded, setHasResponded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // answers keyed by question index → selected label(s)
  const [answers, setAnswers] = useState<Record<number, string[]>>({});

  const { enableScope, disableScope, activeScopes } = useHotkeysContext();
  const tabNav = useContext(TabNavContext);
  const isLogsTabActive = tabNav ? tabNav.activeTab === 'logs' : true;
  const dialogScopeActive = activeScopes.includes(Scope.DIALOG);
  const shouldControlScopes = isLogsTabActive && !dialogScopeActive;
  const approvalsScopeEnabledRef = useRef(false);
  const dialogScopeActiveRef = useRef(dialogScopeActive);

  useEffect(() => {
    dialogScopeActiveRef.current = dialogScopeActive;
  }, [dialogScopeActive]);

  const { timeLeft } = useApprovalCountdown(
    pendingStatus.requested_at,
    pendingStatus.timeout_at,
    hasResponded
  );

  const disabled = isResponding || hasResponded || timeLeft <= 0;

  const shouldEnableApprovalsScope = shouldControlScopes && !disabled;

  useEffect(() => {
    const shouldEnable = shouldEnableApprovalsScope;

    if (shouldEnable && !approvalsScopeEnabledRef.current) {
      enableScope(Scope.APPROVALS);
      disableScope(Scope.KANBAN);
      approvalsScopeEnabledRef.current = true;
    } else if (!shouldEnable && approvalsScopeEnabledRef.current) {
      disableScope(Scope.APPROVALS);
      if (!dialogScopeActive) {
        enableScope(Scope.KANBAN);
      }
      approvalsScopeEnabledRef.current = false;
    }

    return () => {
      if (approvalsScopeEnabledRef.current) {
        disableScope(Scope.APPROVALS);
        if (!dialogScopeActiveRef.current) {
          enableScope(Scope.KANBAN);
        }
        approvalsScopeEnabledRef.current = false;
      }
    };
  }, [
    disableScope,
    enableScope,
    dialogScopeActive,
    shouldEnableApprovalsScope,
  ]);

  const parsedQuestions = useMemo<Question[]>(() => {
    if (!questions || !Array.isArray(questions)) return [];
    return questions as Question[];
  }, [questions]);

  // Check if all questions have at least one answer
  const allAnswered = useMemo(() => {
    if (parsedQuestions.length === 0) return false;
    return parsedQuestions.every(
      (_, idx) => answers[idx] && answers[idx].length > 0
    );
  }, [parsedQuestions, answers]);

  const handleSelect = useCallback(
    (questionIndex: number, values: string[]) => {
      setAnswers((prev) => ({ ...prev, [questionIndex]: values }));
    },
    []
  );

  const handleSubmit = useCallback(async () => {
    if (disabled || !allAnswered) return;
    if (!executionProcessId) {
      setError('Missing executionProcessId');
      return;
    }

    setIsResponding(true);
    setError(null);

    // Build the answers map: { "0": "selected label", "1": "other label" }
    const answersMap: Record<string, string> = {};
    for (const [idx, vals] of Object.entries(answers)) {
      answersMap[idx] = vals.join(', ');
    }

    // Build updated_input: original questions + answers
    const updatedInput: JsonValue = {
      questions: questions as JsonValue,
      answers: answersMap as unknown as JsonValue,
    };

    try {
      await approvalsApi.respond(pendingStatus.approval_id, {
        execution_process_id: executionProcessId,
        status: { status: 'approved' },
        updated_input: updatedInput,
      });
      setHasResponded(true);
    } catch (e: unknown) {
      console.error('AskUserQuestion respond failed:', e);
      const errorMessage =
        e instanceof Error ? e.message : 'Failed to send response';
      setError(errorMessage);
    } finally {
      setIsResponding(false);
    }
  }, [
    disabled,
    allAnswered,
    executionProcessId,
    answers,
    questions,
    pendingStatus.approval_id,
  ]);

  // Auto-submit for single-select, single-question
  const prevAnswersRef = useRef(answers);
  useEffect(() => {
    if (
      parsedQuestions.length === 1 &&
      !parsedQuestions[0].multiSelect &&
      allAnswered &&
      !hasResponded &&
      !isResponding &&
      // Only auto-submit when answers actually changed
      prevAnswersRef.current !== answers
    ) {
      prevAnswersRef.current = answers;
      handleSubmit();
    }
  }, [
    parsedQuestions,
    allAnswered,
    hasResponded,
    isResponding,
    answers,
    handleSubmit,
  ]);

  if (parsedQuestions.length === 0) {
    // Fallback: no parseable questions, show as normal pending approval
    return (
      <div className="relative mt-3">
        <div className="overflow-hidden">{children}</div>
      </div>
    );
  }

  return (
    <div className="relative mt-3">
      <div className="overflow-hidden">
        {children}

        <div className="bg-background px-4 py-3 space-y-4">
          <TooltipProvider>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <MessageCircleQuestion className="h-4 w-4" />
              <span>Claude is asking a question</span>
            </div>

            <div className="space-y-4">
              {parsedQuestions.map((q, idx) => (
                <QuestionCard
                  key={idx}
                  question={q}
                  questionIndex={idx}
                  selected={answers[idx] ?? []}
                  onSelect={handleSelect}
                  disabled={disabled}
                />
              ))}
            </div>

            {/* Show submit button for multi-select or multi-question */}
            {(parsedQuestions.length > 1 ||
              parsedQuestions.some((q) => q.multiSelect)) && (
              <div className="flex justify-end">
                <Button
                  size="sm"
                  onClick={handleSubmit}
                  disabled={disabled || !allAnswered}
                >
                  {isResponding ? 'Submitting...' : 'Submit'}
                </Button>
              </div>
            )}

            {hasResponded && (
              <div className="text-xs text-muted-foreground">
                Answer submitted
              </div>
            )}

            {error && (
              <div
                className="text-xs text-red-600"
                role="alert"
                aria-live="polite"
              >
                {error}
              </div>
            )}
          </TooltipProvider>
        </div>
      </div>
    </div>
  );
};

export default AskUserQuestionEntry;
