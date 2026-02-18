import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import type { OrganizationWithRole } from 'shared/types';
import { AppBarUserPopover } from '../primitives/AppBarUserPopover';
import { SettingsDialog } from '../dialogs/SettingsDialog';
import { useOrganizationStore } from '@/stores/useOrganizationStore';

interface AppBarUserPopoverContainerProps {
  organizations: OrganizationWithRole[];
  selectedOrgId: string;
  onOrgSelect: (orgId: string) => void;
  onCreateOrg: () => void;
}

export function AppBarUserPopoverContainer({
  organizations,
  selectedOrgId,
  onOrgSelect,
  onCreateOrg,
}: AppBarUserPopoverContainerProps) {
  const setSelectedOrgId = useOrganizationStore((s) => s.setSelectedOrgId);
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [avatarError, setAvatarError] = useState(false);

  const handleOrgSettings = async (orgId: string) => {
    setSelectedOrgId(orgId);
    await SettingsDialog.show({ initialSection: 'general' });
  };

  const handleMigrate = () => {
    setOpen(false);
    navigate('/migrate');
  };

  return (
    <AppBarUserPopover
      avatarUrl={null}
      avatarError={avatarError}
      organizations={organizations}
      selectedOrgId={selectedOrgId}
      open={open}
      onOpenChange={setOpen}
      onOrgSelect={onOrgSelect}
      onCreateOrg={onCreateOrg}
      onOrgSettings={handleOrgSettings}
      onAvatarError={() => setAvatarError(true)}
      onMigrate={handleMigrate}
    />
  );
}
