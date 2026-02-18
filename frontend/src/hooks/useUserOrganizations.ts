import { useQuery } from '@tanstack/react-query';
import { organizationsApi } from '../lib/api';
import type { ListOrganizationsResponse } from 'shared/types';
import { organizationKeys } from './organizationKeys';

/**
 * Hook to fetch all organizations that the current user is a member of
 */
export function useUserOrganizations() {
  return useQuery<ListOrganizationsResponse>({
    queryKey: organizationKeys.userList(),
    queryFn: () => organizationsApi.getUserOrganizations(),
    enabled: false, // Auth is not available in local-only mode
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}
