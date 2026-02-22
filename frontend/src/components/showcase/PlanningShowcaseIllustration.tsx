import {
  Lightbulb,
  MessageSquare,
  FileText,
  GitBranch,
  Bot,
  ArrowRight,
  CheckCircle2,
  ListChecks,
  Layers,
} from 'lucide-react';

type PlanningStep =
  | 'overview'
  | 'createEpic'
  | 'planWithClaude'
  | 'specAndDecompose'
  | 'ralph';

interface PlanningShowcaseIllustrationProps {
  step: PlanningStep;
}

const stepConfig: Record<
  PlanningStep,
  {
    icon: React.ElementType;
    iconColor: string;
    iconBg: string;
    label: string;
    subItems?: { icon: React.ElementType; label: string }[];
  }
> = {
  overview: {
    icon: Layers,
    iconColor: 'text-purple-500',
    iconBg: 'bg-purple-100 dark:bg-purple-900/40',
    label: 'Planning Board',
    subItems: [
      { icon: Lightbulb, label: 'Idea' },
      { icon: MessageSquare, label: 'Planning' },
      { icon: FileText, label: 'Spec Review' },
      { icon: CheckCircle2, label: 'Ready' },
      { icon: Bot, label: 'Ralph' },
    ],
  },
  createEpic: {
    icon: Lightbulb,
    iconColor: 'text-amber-500',
    iconBg: 'bg-amber-100 dark:bg-amber-900/40',
    label: 'Create Epic',
    subItems: [
      { icon: FileText, label: 'Title & Description' },
      { icon: Lightbulb, label: 'Lands in Idea column' },
    ],
  },
  planWithClaude: {
    icon: MessageSquare,
    iconColor: 'text-blue-500',
    iconBg: 'bg-blue-100 dark:bg-blue-900/40',
    label: 'Plan with Claude Code',
    subItems: [
      { icon: MessageSquare, label: 'Link planning sessions' },
      { icon: FileText, label: 'Iterate on scope & approach' },
      { icon: ArrowRight, label: 'Move to Spec Review when ready' },
    ],
  },
  specAndDecompose: {
    icon: ListChecks,
    iconColor: 'text-green-500',
    iconBg: 'bg-green-100 dark:bg-green-900/40',
    label: 'Spec & Decompose',
    subItems: [
      { icon: FileText, label: 'Generate spec sheet with AI' },
      { icon: GitBranch, label: 'Decompose into child stories' },
      { icon: CheckCircle2, label: 'Review & approve the breakdown' },
    ],
  },
  ralph: {
    icon: Bot,
    iconColor: 'text-violet-500',
    iconBg: 'bg-violet-100 dark:bg-violet-900/40',
    label: 'Ralph Executes',
    subItems: [
      { icon: Bot, label: 'Ralph picks up child stories' },
      { icon: GitBranch, label: 'Each story gets its own branch' },
      { icon: CheckCircle2, label: 'Automatic PR & completion' },
    ],
  },
};

export function PlanningShowcaseIllustration({
  step,
}: PlanningShowcaseIllustrationProps) {
  const config = stepConfig[step];
  const Icon = config.icon;

  return (
    <div className="flex flex-col items-center justify-center gap-6 p-8">
      <div
        className={`flex h-20 w-20 items-center justify-center rounded-2xl ${config.iconBg}`}
      >
        <Icon className={`h-10 w-10 ${config.iconColor}`} />
      </div>
      <span className="text-lg font-semibold text-foreground">
        {config.label}
      </span>
      {config.subItems && (
        <div className="flex flex-col gap-3 w-full max-w-xs">
          {config.subItems.map((item, i) => {
            const SubIcon = item.icon;
            return (
              <div
                key={i}
                className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-2.5"
              >
                <SubIcon className="h-4 w-4 text-muted-foreground shrink-0" />
                <span className="text-sm text-foreground">{item.label}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
