import { useState, useCallback, useRef, KeyboardEvent } from 'react';
import { Button } from '@/components/ui/button';
import { Send, Square } from 'lucide-react';
import { sessionsApi } from '@/lib/api';
import { useExecutionProcessesContext } from '@/contexts/ExecutionProcessesContext';
import type { BaseCodingAgent } from 'shared/types';

interface PlanEditChatInputProps {
  sessionId: string;
  executor: BaseCodingAgent;
  disabled?: boolean;
}

export function PlanEditChatInput({
  sessionId,
  executor,
  disabled,
}: PlanEditChatInputProps) {
  const [message, setMessage] = useState('');
  const [isSending, setIsSending] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { isAttemptRunningVisible } = useExecutionProcessesContext();

  const handleSend = useCallback(async () => {
    const trimmed = message.trim();
    if (!trimmed || isSending || isAttemptRunningVisible) return;

    setIsSending(true);
    try {
      await sessionsApi.followUp(sessionId, {
        prompt: trimmed,
        executor_profile_id: { executor, variant: null },
        retry_process_id: null,
        force_when_dirty: null,
        perform_git_reset: null,
      });
      setMessage('');
    } catch {
      // Error handled by streaming context
    } finally {
      setIsSending(false);
    }
  }, [message, isSending, isAttemptRunningVisible, sessionId, executor]);

  const handleStop = useCallback(async () => {
    // Stop is handled at the execution process level
    // The user can use the stop button that appears in the conversation
  }, []);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  const canSend =
    message.trim().length > 0 && !isSending && !isAttemptRunningVisible;

  return (
    <div className="flex items-end gap-2 border-t p-3">
      <textarea
        ref={textareaRef}
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Ask Claude to refine the plan..."
        disabled={disabled || isSending}
        rows={2}
        className="flex-1 resize-none rounded-md border bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
      />
      {isAttemptRunningVisible ? (
        <Button size="icon" variant="outline" onClick={handleStop} title="Stop">
          <Square className="h-4 w-4" />
        </Button>
      ) : (
        <Button
          size="icon"
          onClick={handleSend}
          disabled={!canSend}
          title="Send (Cmd+Enter)"
        >
          <Send className="h-4 w-4" />
        </Button>
      )}
    </div>
  );
}
