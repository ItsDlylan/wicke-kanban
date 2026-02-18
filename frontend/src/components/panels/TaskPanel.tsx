import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/contexts/ProjectContext';
import { useTaskAttemptsWithSessions } from '@/hooks/useTaskAttempts';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { useTaskChildren } from '@/hooks/useTaskChildren';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import type { TaskWithAttemptStatus, ChildTaskWithDeps } from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';
import { NewCardContent } from '../ui/new-card';
import { Button } from '../ui/button';
import { PlusIcon } from 'lucide-react';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { DataTable, type ColumnDef } from '@/components/ui/table';
import { statusLabels } from '@/utils/statusLabels';

interface TaskPanelProps {
  task: TaskWithAttemptStatus | null;
}

const statusBadgeColors: Record<string, string> = {
  done: 'bg-green-200 text-green-800 dark:bg-green-800 dark:text-green-200',
  inprogress: 'bg-blue-200 text-blue-800 dark:bg-blue-800 dark:text-blue-200',
  ralph: 'bg-blue-200 text-blue-800 dark:bg-blue-800 dark:text-blue-200',
  ready: 'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-200',
  backlog: 'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-200',
  qa: 'bg-yellow-200 text-yellow-800 dark:bg-yellow-800 dark:text-yellow-200',
  cancelled: 'bg-red-200 text-red-800 dark:bg-red-800 dark:text-red-200',
  plangenerating:
    'bg-purple-200 text-purple-800 dark:bg-purple-800 dark:text-purple-200',
};

function StatusBadge({ status }: { status: string }) {
  const colors =
    statusBadgeColors[status] ??
    'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-200';
  const label = statusLabels[status as keyof typeof statusLabels] ?? status;
  return (
    <span
      className={`px-1.5 py-0.5 text-xs rounded whitespace-nowrap ${colors}`}
    >
      {label}
    </span>
  );
}

const TaskPanel = ({ task }: TaskPanelProps) => {
  const { t } = useTranslation('tasks');
  const navigate = useNavigateWithSearch();
  const { projectId } = useProject();

  const {
    data: attempts = [],
    isLoading: isAttemptsLoading,
    isError: isAttemptsError,
  } = useTaskAttemptsWithSessions(task?.id);

  const { data: parentAttempt, isLoading: isParentLoading } =
    useTaskAttemptWithSession(task?.parent_workspace_id || undefined);

  const { data: children, isLoading: isChildrenLoading } = useTaskChildren(
    task?.has_children ? task.id : undefined
  );

  const childrenProgress = useMemo(() => {
    if (!children || children.length === 0) return null;
    const done = children.filter((c) => c.status === 'done').length;
    return { done, total: children.length };
  }, [children]);

  const formatTimeAgo = (iso: string) => {
    const d = new Date(iso);
    const diffMs = Date.now() - d.getTime();
    const absSec = Math.round(Math.abs(diffMs) / 1000);

    const rtf =
      typeof Intl !== 'undefined' &&
      typeof Intl.RelativeTimeFormat === 'function'
        ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
        : null;

    const to = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
      rtf
        ? rtf.format(-value, unit)
        : `${value} ${unit}${value !== 1 ? 's' : ''} ago`;

    if (absSec < 60) return to(Math.round(absSec), 'second');
    const mins = Math.round(absSec / 60);
    if (mins < 60) return to(mins, 'minute');
    const hours = Math.round(mins / 60);
    if (hours < 24) return to(hours, 'hour');
    const days = Math.round(hours / 24);
    if (days < 30) return to(days, 'day');
    const months = Math.round(days / 30);
    if (months < 12) return to(months, 'month');
    const years = Math.round(months / 12);
    return to(years, 'year');
  };

  const displayedAttempts = [...attempts].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );

  if (!task) {
    return (
      <div className="text-muted-foreground">
        {t('taskPanel.noTaskSelected')}
      </div>
    );
  }

  const titleContent = `# ${task.title || 'Task'}`;
  const descriptionContent = task.description || '';

  const childColumns: ColumnDef<ChildTaskWithDeps>[] = [
    {
      id: 'status',
      header: '',
      accessor: (child) => <StatusBadge status={child.status} />,
      className: 'pr-2 w-0',
    },
    {
      id: 'title',
      header: '',
      accessor: (child) => (
        <span className="truncate block max-w-[20rem]">{child.title}</span>
      ),
      className: 'pr-4',
    },
  ];

  const attemptColumns: ColumnDef<WorkspaceWithSession>[] = [
    {
      id: 'executor',
      header: '',
      accessor: (attempt) => attempt.session?.executor || 'Base Agent',
      className: 'pr-4',
    },
    {
      id: 'branch',
      header: '',
      accessor: (attempt) => attempt.branch || '—',
      className: 'pr-4',
    },
    {
      id: 'time',
      header: '',
      accessor: (attempt) => formatTimeAgo(attempt.created_at),
      className: 'pr-0 text-right',
    },
  ];

  return (
    <>
      <NewCardContent>
        <div className="p-6 flex flex-col h-full max-h-[calc(100vh-8rem)]">
          <div className="space-y-3 overflow-y-auto flex-shrink min-h-0">
            <WYSIWYGEditor value={titleContent} disabled />
            {descriptionContent && (
              <WYSIWYGEditor value={descriptionContent} disabled />
            )}
          </div>

          <div className="mt-6 flex-shrink-0 space-y-4">
            {task.has_children && (
              <DataTable
                data={children ?? []}
                columns={childColumns}
                keyExtractor={(child) => child.id}
                onRowClick={(child) => {
                  if (projectId) {
                    navigate(
                      `${paths.task(projectId, child.id)}/attempts/latest`
                    );
                  }
                }}
                isLoading={isChildrenLoading}
                headerContent={
                  childrenProgress
                    ? `Stories: ${childrenProgress.done}/${childrenProgress.total} complete`
                    : 'Stories'
                }
              />
            )}

            {task.parent_workspace_id && (
              <DataTable
                data={parentAttempt ? [parentAttempt] : []}
                columns={attemptColumns}
                keyExtractor={(attempt) => attempt.id}
                onRowClick={(attempt) => {
                  if (projectId) {
                    navigate(
                      paths.attempt(projectId, attempt.task_id, attempt.id)
                    );
                  }
                }}
                isLoading={isParentLoading}
                headerContent="Parent Attempt"
              />
            )}

            {isAttemptsLoading ? (
              <div className="text-muted-foreground">
                {t('taskPanel.loadingAttempts')}
              </div>
            ) : isAttemptsError ? (
              <div className="text-destructive">
                {t('taskPanel.errorLoadingAttempts')}
              </div>
            ) : (
              <DataTable
                data={displayedAttempts}
                columns={attemptColumns}
                keyExtractor={(attempt) => attempt.id}
                onRowClick={(attempt) => {
                  if (projectId && task.id) {
                    navigate(paths.attempt(projectId, task.id, attempt.id));
                  }
                }}
                emptyState={t('taskPanel.noAttempts')}
                headerContent={
                  <div className="w-full flex text-left">
                    <span className="flex-1">
                      {t('taskPanel.attemptsCount', {
                        count: displayedAttempts.length,
                      })}
                    </span>
                    <span>
                      <Button
                        variant="icon"
                        onClick={() =>
                          CreateAttemptDialog.show({
                            taskId: task.id,
                          })
                        }
                      >
                        <PlusIcon size={16} />
                      </Button>
                    </span>
                  </div>
                }
              />
            )}
          </div>
        </div>
      </NewCardContent>
    </>
  );
};

export default TaskPanel;
