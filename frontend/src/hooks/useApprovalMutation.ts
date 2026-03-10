import { useMutation } from '@tanstack/react-query';
import { approvalsApi } from '@/lib/api';

interface ApproveParams {
  approvalId: string;
  executionProcessId: string;
}

interface DenyParams extends ApproveParams {
  reason?: string;
}

export function useApprovalMutation() {
  const approveMutation = useMutation({
    mutationFn: ({ approvalId, executionProcessId }: ApproveParams) =>
      approvalsApi.respond(approvalId, {
        execution_process_id: executionProcessId,
        status: { status: 'approved' },
      }),
    onError: (err, variables) => {
      console.error('[useApprovalMutation.approve] Failed to approve:', {
        err,
        approvalId: variables.approvalId,
        processId: variables.executionProcessId,
      });
    },
  });

  const denyMutation = useMutation({
    mutationFn: ({ approvalId, executionProcessId, reason }: DenyParams) =>
      approvalsApi.respond(approvalId, {
        execution_process_id: executionProcessId,
        status: {
          status: 'denied',
          reason: reason || 'User denied this request.',
        },
      }),
    onError: (err, variables) => {
      console.error('[useApprovalMutation.deny] Failed to deny:', {
        err,
        approvalId: variables.approvalId,
        processId: variables.executionProcessId,
      });
    },
  });

  return {
    approve: approveMutation.mutate,
    approveAsync: approveMutation.mutateAsync,
    deny: denyMutation.mutate,
    denyAsync: denyMutation.mutateAsync,
    isApproving: approveMutation.isPending,
    isDenying: denyMutation.isPending,
    isResponding: approveMutation.isPending || denyMutation.isPending,
    approveError: approveMutation.error,
    denyError: denyMutation.error,
    reset: () => {
      approveMutation.reset();
      denyMutation.reset();
    },
  };
}
