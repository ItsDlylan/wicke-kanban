import { MigrateIntroduction } from '@/components/ui-new/views/MigrateIntroduction';

interface MigrateIntroductionContainerProps {
  onContinue: () => void;
}

export function MigrateIntroductionContainer({
  onContinue,
}: MigrateIntroductionContainerProps) {
  return <MigrateIntroduction onAction={onContinue} />;
}
