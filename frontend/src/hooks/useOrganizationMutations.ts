import { useMutation, useQueryClient } from '@tanstack/react-query';
import { organizationsApi } from '@/lib/api';
import type {
  MemberRole,
  UpdateMemberRoleResponse,
  CreateOrganizationRequest,
  CreateOrganizationResponse,
  CreateInvitationRequest,
  CreateInvitationResponse,
  ListOrganizationsResponse,
} from 'shared/types';
import { organizationKeys } from './organizationKeys';

interface UseOrganizationMutationsOptions {
  onCreateSuccess?: (result: CreateOrganizationResponse) => void;
  onCreateError?: (err: unknown) => void;
  onInviteSuccess?: (result: CreateInvitationResponse) => void;
  onInviteError?: (err: unknown) => void;
  onRevokeSuccess?: () => void;
  onRevokeError?: (err: unknown) => void;
  onRemoveSuccess?: () => void;
  onRemoveError?: (err: unknown) => void;
  onRoleChangeSuccess?: () => void;
  onRoleChangeError?: (err: unknown) => void;
  onDeleteSuccess?: () => void;
  onDeleteError?: (err: unknown) => void;
}

export function useOrganizationMutations(
  options?: UseOrganizationMutationsOptions
) {
  const queryClient = useQueryClient();

  const createOrganization = useMutation({
    mutationKey: ['createOrganization'],
    mutationFn: (data: CreateOrganizationRequest) =>
      organizationsApi.createOrganization(data),
    onSuccess: (result: CreateOrganizationResponse) => {
      // Immediately add new org to cache to prevent race condition with selection
      queryClient.setQueryData<ListOrganizationsResponse>(
        organizationKeys.userList(),
        (old) => {
          if (!old) return { organizations: [result.organization] };
          return {
            organizations: [...old.organizations, result.organization],
          };
        }
      );

      // Then invalidate to ensure server data stays fresh
      queryClient.invalidateQueries({ queryKey: organizationKeys.userList() });
      options?.onCreateSuccess?.(result);
    },
    onError: (err) => {
      console.error(
        '[useOrganizationMutations] Failed to create organization',
        {
          err,
        }
      );
      options?.onCreateError?.(err);
    },
  });

  const createInvitation = useMutation({
    mutationKey: ['createInvitation'],
    mutationFn: ({
      orgId,
      data,
    }: {
      orgId: string;
      data: CreateInvitationRequest;
    }) => organizationsApi.createInvitation(orgId, data),
    onSuccess: (result: CreateInvitationResponse, variables) => {
      queryClient.invalidateQueries({
        queryKey: organizationKeys.members(variables.orgId),
      });
      queryClient.invalidateQueries({
        queryKey: organizationKeys.invitations(variables.orgId),
      });
      options?.onInviteSuccess?.(result);
    },
    onError: (err, variables) => {
      console.error('[useOrganizationMutations] Failed to create invitation', {
        err,
        orgId: variables.orgId,
      });
      options?.onInviteError?.(err);
    },
  });

  const revokeInvitation = useMutation({
    mutationFn: ({
      orgId,
      invitationId,
    }: {
      orgId: string;
      invitationId: string;
    }) => organizationsApi.revokeInvitation(orgId, invitationId),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: organizationKeys.members(variables.orgId),
      });
      queryClient.invalidateQueries({
        queryKey: organizationKeys.invitations(variables.orgId),
      });
      options?.onRevokeSuccess?.();
    },
    onError: (err, variables) => {
      console.error('[useOrganizationMutations] Failed to revoke invitation', {
        err,
        orgId: variables.orgId,
        invitationId: variables.invitationId,
      });
      options?.onRevokeError?.(err);
    },
  });

  const removeMember = useMutation({
    mutationFn: ({ orgId, userId }: { orgId: string; userId: string }) =>
      organizationsApi.removeMember(orgId, userId),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: organizationKeys.members(variables.orgId),
      });
      // Invalidate user's organizations in case we removed ourselves
      queryClient.invalidateQueries({ queryKey: organizationKeys.userList() });
      options?.onRemoveSuccess?.();
    },
    onError: (err, variables) => {
      console.error('[useOrganizationMutations] Failed to remove member', {
        err,
        orgId: variables.orgId,
        userId: variables.userId,
      });
      options?.onRemoveError?.(err);
    },
  });

  const updateMemberRole = useMutation<
    UpdateMemberRoleResponse,
    unknown,
    { orgId: string; userId: string; role: MemberRole }
  >({
    mutationFn: ({ orgId, userId, role }) =>
      organizationsApi.updateMemberRole(orgId, userId, { role }),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: organizationKeys.members(variables.orgId),
      });
      // Invalidate user's organizations in case we changed our own role
      queryClient.invalidateQueries({ queryKey: organizationKeys.userList() });
      options?.onRoleChangeSuccess?.();
    },
    onError: (err, variables) => {
      console.error('[useOrganizationMutations] Failed to update member role', {
        err,
        orgId: variables.orgId,
        userId: variables.userId,
        role: variables.role,
      });
      options?.onRoleChangeError?.(err);
    },
  });

  const refetchMembers = async (orgId: string) => {
    await queryClient.invalidateQueries({
      queryKey: organizationKeys.members(orgId),
    });
  };

  const refetchInvitations = async (orgId: string) => {
    await queryClient.invalidateQueries({
      queryKey: organizationKeys.invitations(orgId),
    });
  };

  const deleteOrganization = useMutation({
    mutationFn: (orgId: string) => organizationsApi.deleteOrganization(orgId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: organizationKeys.userList() });
      options?.onDeleteSuccess?.();
    },
    onError: (err, orgId) => {
      console.error(
        '[useOrganizationMutations] Failed to delete organization',
        {
          err,
          orgId,
        }
      );
      options?.onDeleteError?.(err);
    },
  });

  return {
    createOrganization,
    createInvitation,
    revokeInvitation,
    removeMember,
    updateMemberRole,
    deleteOrganization,
    refetchMembers,
    refetchInvitations,
  };
}
