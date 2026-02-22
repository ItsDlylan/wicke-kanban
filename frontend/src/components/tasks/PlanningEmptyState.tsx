import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import {
  Lightbulb,
  FileText,
  SearchCheck,
  CheckCircle2,
  Bot,
  PartyPopper,
  Plus,
  ArrowRight,
} from 'lucide-react';

const WORKFLOW_STEPS = [
  { key: 'idea', icon: Lightbulb, color: 'text-yellow-500' },
  { key: 'planning', icon: FileText, color: 'text-blue-500' },
  { key: 'specreview', icon: SearchCheck, color: 'text-purple-500' },
  { key: 'ready', icon: CheckCircle2, color: 'text-green-500' },
  { key: 'ralph', icon: Bot, color: 'text-orange-500' },
  { key: 'done', icon: PartyPopper, color: 'text-emerald-500' },
] as const;

interface PlanningEmptyStateProps {
  onCreateEpic: () => void;
}

export function PlanningEmptyState({ onCreateEpic }: PlanningEmptyStateProps) {
  const { t } = useTranslation('tasks');

  return (
    <div className="max-w-3xl mx-auto mt-8 px-4">
      <Card>
        <CardContent className="py-10 px-8">
          <div className="text-center mb-8">
            <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-muted mb-4">
              <Lightbulb className="h-6 w-6 text-yellow-500" />
            </div>
            <h3 className="text-lg font-semibold">
              {t('planning.empty.title')}
            </h3>
            <p className="mt-2 text-sm text-muted-foreground max-w-lg mx-auto">
              {t('planning.empty.description')}
            </p>
          </div>

          <div className="mb-8">
            <h4 className="text-sm font-medium text-muted-foreground mb-4 text-center">
              {t('planning.empty.workflow.title')}
            </h4>
            <div className="flex items-start justify-center gap-2 flex-wrap">
              {WORKFLOW_STEPS.map((step, index) => {
                const Icon = step.icon;
                return (
                  <div key={step.key} className="flex items-start gap-2">
                    <div className="flex flex-col items-center text-center w-20">
                      <div className="flex h-10 w-10 items-center justify-center rounded-full bg-muted mb-1.5">
                        <Icon className={`h-5 w-5 ${step.color}`} />
                      </div>
                      <span className="text-xs font-medium">
                        {t(`planning.empty.workflow.${step.key}`)}
                      </span>
                      <span className="text-[10px] text-muted-foreground leading-tight mt-0.5">
                        {t(`planning.empty.workflow.${step.key}Description`)}
                      </span>
                    </div>
                    {index < WORKFLOW_STEPS.length - 1 && (
                      <ArrowRight className="h-4 w-4 text-muted-foreground mt-3 shrink-0" />
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          <div className="text-center">
            <Button onClick={onCreateEpic}>
              <Plus className="h-4 w-4 mr-2" />
              {t('planning.empty.createFirst')}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
