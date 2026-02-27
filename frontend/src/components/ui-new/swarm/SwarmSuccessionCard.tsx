import { useState } from 'react';
import { cn } from '@/lib/utils';
import type { SwarmAgent, SwarmSuccession } from 'shared/types';

const PIPELINE_STEPS = ['Assessment', 'Verifying', 'Confidence', 'Successor'];

function getPipelineStep(status: SwarmSuccession['status']): number {
  switch (status) {
    case 'pending':
      return 0;
    case 'verifying':
      return 1;
    case 'verified':
      return 2;
    case 'successor_running':
      return 3;
    case 'failed':
      return -1;
    default:
      return 0;
  }
}

const RECOVERY_LABELS: Record<string, string> = {
  corrective: 'Continue and fix issues',
  clean_restart: 'Start fresh',
  redecomposition: 'Break into subtasks',
  escalation: 'Flag for review',
};

interface SwarmSuccessionCardProps {
  succession: SwarmSuccession;
  agents?: SwarmAgent[];
  onAgentSelect?: (executionProcessId: string) => void;
}

export function SwarmSuccessionCard({
  succession,
  agents = [],
  onAgentSelect,
}: SwarmSuccessionCardProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [showAssessment, setShowAssessment] = useState(false);
  const confidence = succession.verifier_confidence;
  const activeStep = getPipelineStep(succession.status);

  const predecessor = agents.find((a) => a.id === succession.predecessor_id);
  const successor = succession.successor_id
    ? agents.find((a) => a.id === succession.successor_id)
    : null;

  const handleAgentClick = (
    e: React.MouseEvent,
    agent: SwarmAgent | undefined
  ) => {
    e.stopPropagation();
    if (agent?.execution_process_id && onAgentSelect) {
      onAgentSelect(agent.execution_process_id);
    }
  };

  return (
    <div className="rounded border border-white/5 bg-secondary p-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 text-xs text-low">
          <span>Succession</span>
          <span className="text-low/50">&rarr;</span>
          <span
            className={cn(
              'rounded-full px-1.5 py-0.5 font-medium',
              succession.status === 'verified' ||
                succession.status === 'successor_running'
                ? 'bg-green-500/20 text-green-400'
                : succession.status === 'failed'
                  ? 'bg-red-500/20 text-red-400'
                  : 'bg-secondary text-low'
            )}
          >
            {succession.status.replace('_', ' ')}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          {confidence != null && (
            <span
              className={cn(
                'text-xs font-medium',
                confidence >= 0.7
                  ? 'text-green-400'
                  : confidence >= 0.3
                    ? 'text-yellow-400'
                    : 'text-red-400'
              )}
            >
              {Math.round(confidence * 100)}%
            </span>
          )}
          {succession.recovery_strategy && (
            <span className="text-xs text-low">
              {RECOVERY_LABELS[succession.recovery_strategy] ??
                succession.recovery_strategy}
            </span>
          )}
        </div>
      </div>

      {/* Pipeline visualization */}
      <div className="mt-2 flex items-center gap-1">
        {PIPELINE_STEPS.map((step, i) => {
          const isFailed = succession.status === 'failed';
          const isActive = !isFailed && i <= activeStep;
          const isCurrent = !isFailed && i === activeStep;
          return (
            <div key={step} className="flex flex-1 flex-col items-center">
              <div
                className={cn(
                  'h-1 w-full rounded-full',
                  isFailed && i <= 1
                    ? 'bg-red-500/40'
                    : isActive
                      ? 'bg-blue-400'
                      : 'bg-white/10'
                )}
              />
              <span
                className={cn(
                  'mt-0.5 text-[10px]',
                  isCurrent ? 'text-normal' : 'text-low/50'
                )}
              >
                {step}
              </span>
            </div>
          );
        })}
      </div>

      {/* Predecessor / Successor links */}
      <div className="mt-1.5 flex items-center gap-2 text-xs text-low">
        {predecessor && (
          <button
            className="truncate hover:underline"
            onClick={(e) => handleAgentClick(e, predecessor)}
          >
            From: {predecessor.subtask_description.slice(0, 30)}
            {predecessor.subtask_description.length > 30 ? '...' : ''}
          </button>
        )}
        {successor && (
          <>
            <span className="text-low/50">&rarr;</span>
            <button
              className="truncate hover:underline"
              onClick={(e) => handleAgentClick(e, successor)}
            >
              To: {successor.subtask_description.slice(0, 30)}
              {successor.subtask_description.length > 30 ? '...' : ''}
            </button>
          </>
        )}
      </div>

      {/* Self-assessment toggle */}
      {succession.predecessor_self_assessment && (
        <button
          className="mt-1 text-xs text-low underline-offset-2 hover:underline"
          onClick={() => setShowAssessment(!showAssessment)}
        >
          {showAssessment ? 'Hide' : 'Show'} self-assessment
        </button>
      )}

      {showAssessment && succession.predecessor_self_assessment && (
        <pre className="mt-1.5 max-h-40 overflow-auto rounded bg-primary p-2 text-xs text-low">
          {succession.predecessor_self_assessment}
        </pre>
      )}

      {/* Verification report toggle */}
      {succession.verification_report && (
        <button
          className="mt-1 text-xs text-low underline-offset-2 hover:underline"
          onClick={() => setIsExpanded(!isExpanded)}
        >
          {isExpanded ? 'Hide' : 'Show'} verification report
        </button>
      )}

      {isExpanded && succession.verification_report && (
        <pre className="mt-1.5 max-h-40 overflow-auto rounded bg-primary p-2 text-xs text-low">
          {succession.verification_report}
        </pre>
      )}
    </div>
  );
}
