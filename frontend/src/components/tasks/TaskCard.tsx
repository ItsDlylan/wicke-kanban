import { useCallback, useEffect, useRef, useState } from 'react';
import { KanbanCard } from '@/components/ui/shadcn-io/kanban';
import {
  CheckCircle2,
  FileText,
  GitBranch,
  Link,
  Loader2,
  XCircle,
} from 'lucide-react';
import type { TaskWithAttemptStatus } from 'shared/types';
import { ActionsDropdown } from '@/components/ui/actions-dropdown';
import { Button } from '@/components/ui/button';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import { attemptsApi } from '@/lib/api';
import { TaskCardHeader } from './TaskCardHeader';
import { useTranslation } from 'react-i18next';

type Task = TaskWithAttemptStatus;

interface TaskCardProps {
  task: Task;
  index: number;
  status: string;
  onViewDetails: (task: Task) => void;
  isOpen?: boolean;
  projectId: string;
}

export function TaskCard({
  task,
  index,
  status,
  onViewDetails,
  isOpen,
  projectId,
}: TaskCardProps) {
  const { t } = useTranslation('tasks');
  const navigate = useNavigateWithSearch();
  const [isNavigatingToParent, setIsNavigatingToParent] = useState(false);

  const handleClick = useCallback(() => {
    onViewDetails(task);
  }, [task, onViewDetails]);

  const handleParentClick = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!task.parent_workspace_id || isNavigatingToParent) return;

      setIsNavigatingToParent(true);
      try {
        const parentAttempt = await attemptsApi.get(task.parent_workspace_id);
        navigate(
          paths.attempt(
            projectId,
            parentAttempt.task_id,
            task.parent_workspace_id
          )
        );
      } catch (error) {
        console.error('Failed to navigate to parent task attempt:', error);
        setIsNavigatingToParent(false);
      }
    },
    [task.parent_workspace_id, projectId, navigate, isNavigatingToParent]
  );

  const localRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen || !localRef.current) return;
    const el = localRef.current;
    requestAnimationFrame(() => {
      el.scrollIntoView({
        block: 'center',
        inline: 'nearest',
        behavior: 'smooth',
      });
    });
  }, [isOpen]);

  const isEpic = task.has_children;
  const isRalphReady = task.has_spec && task.has_children;

  return (
    <KanbanCard
      key={task.id}
      id={task.id}
      name={task.title}
      index={index}
      parent={status}
      onClick={handleClick}
      isOpen={isOpen}
      forwardedRef={localRef}
      className={
        isEpic
          ? 'border-l-2 border-l-purple-500 bg-purple-50/50 dark:bg-purple-950/20'
          : undefined
      }
    >
      <div className="flex flex-col gap-2">
        <TaskCardHeader
          title={task.title}
          right={
            <>
              {task.plan_status === 'generating' && (
                <span title="Generating plan...">
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-yellow-500" />
                </span>
              )}
              {task.plan_status === 'completed' && (
                <span title="Plan ready">
                  <CheckCircle2 className="h-3.5 w-3.5 text-green-500" />
                </span>
              )}
              {task.plan_status === 'failed' && (
                <span title="Plan failed">
                  <XCircle className="h-3.5 w-3.5 text-red-500" />
                </span>
              )}
              {isRalphReady && (
                <span
                  title="Ralph Ready"
                  className="text-[10px] font-bold text-green-600 bg-green-100 dark:bg-green-900/30 dark:text-green-400 px-1 rounded"
                >
                  R
                </span>
              )}
              {task.has_in_progress_attempt && (
                <Loader2 className="h-4 w-4 animate-spin text-blue-500" />
              )}
              {task.last_attempt_failed && (
                <XCircle className="h-4 w-4 text-destructive" />
              )}
              {task.parent_workspace_id && (
                <Button
                  variant="icon"
                  onClick={handleParentClick}
                  onPointerDown={(e) => e.stopPropagation()}
                  onMouseDown={(e) => e.stopPropagation()}
                  disabled={isNavigatingToParent}
                  title={t('navigateToParent')}
                >
                  <Link className="h-4 w-4" />
                </Button>
              )}
              <ActionsDropdown task={task} />
            </>
          }
        />
        {isEpic && (
          <div className="flex items-center gap-2">
            {task.has_spec && (
              <span
                title="Spec sheet"
                className="inline-flex items-center gap-0.5 text-[10px] text-purple-600 dark:text-purple-400"
              >
                <FileText className="h-3 w-3" />
                <span>Spec</span>
              </span>
            )}
            <span
              title="Epic — has subtasks"
              className="inline-flex items-center gap-0.5 text-[10px] text-purple-600 dark:text-purple-400"
            >
              <GitBranch className="h-3 w-3" />
              <span>Epic</span>
            </span>
          </div>
        )}
        {task.description && (
          <p className="text-sm text-secondary-foreground break-words">
            {task.description.length > 130
              ? `${task.description.substring(0, 130)}...`
              : task.description}
          </p>
        )}
      </div>
    </KanbanCard>
  );
}
