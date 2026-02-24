import { useState } from 'react';
import { cn } from '@/lib/utils';
import type { SwarmSuccession } from 'shared/types';

interface SwarmSuccessionCardProps {
  succession: SwarmSuccession;
}

export function SwarmSuccessionCard({ succession }: SwarmSuccessionCardProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const confidence = succession.verifier_confidence;

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
              {succession.recovery_strategy}
            </span>
          )}
        </div>
      </div>

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
