import { Card } from '@/components/ui/card.tsx';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu.tsx';
import { Button } from '@/components/ui/button.tsx';
import {
  Edit,
  ExternalLink,
  FolderOpen,
  GitCommitHorizontal,
  Minus,
  MoreHorizontal,
  Plus,
  Trash2,
} from 'lucide-react';
import type { Project, ProjectStats } from 'shared/types';
import { useEffect, useMemo, useRef } from 'react';
import { useOpenProjectInEditor } from '@/hooks/useOpenProjectInEditor';
import { useNavigateWithSearch, useProjectRepos } from '@/hooks';
import { projectsApi } from '@/lib/api';
import { useTranslation } from 'react-i18next';

type Props = {
  project: Project;
  stats?: ProjectStats;
  isFocused: boolean;
  setError: (error: string) => void;
  onEdit: (project: Project) => void;
};

const GRADIENTS = [
  'from-violet-500 to-purple-600',
  'from-blue-500 to-cyan-500',
  'from-emerald-500 to-teal-600',
  'from-orange-500 to-amber-500',
  'from-rose-500 to-pink-600',
  'from-indigo-500 to-blue-600',
  'from-fuchsia-500 to-purple-500',
  'from-sky-500 to-indigo-500',
  'from-lime-500 to-emerald-500',
  'from-red-500 to-orange-500',
];

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash);
}

function formatRelativeTime(dateStr: string | null): string {
  if (!dateStr) return 'No recent activity';
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60_000);
  const diffHours = Math.floor(diffMs / 3_600_000);
  const diffDays = Math.floor(diffMs / 86_400_000);

  if (diffMins < 1) return 'Just now';
  if (diffMins < 60) return `${diffMins} min${diffMins === 1 ? '' : 's'} ago`;
  if (diffHours < 24)
    return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`;
  if (diffDays === 1) return 'Yesterday';
  if (diffDays < 30) return `${diffDays} days ago`;
  return date.toLocaleDateString();
}

function ProjectCard({ project, stats, isFocused, setError, onEdit }: Props) {
  const navigate = useNavigateWithSearch();
  const ref = useRef<HTMLDivElement>(null);
  const handleOpenInEditor = useOpenProjectInEditor(project);
  const { t } = useTranslation('projects');

  const { data: repos } = useProjectRepos(project.id);
  const isSingleRepoProject = repos?.length === 1;

  const gradientClass = useMemo(
    () => GRADIENTS[hashString(project.name) % GRADIENTS.length],
    [project.name]
  );

  useEffect(() => {
    if (isFocused && ref.current) {
      ref.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      ref.current.focus();
    }
  }, [isFocused]);

  const handleDelete = async (id: string, name: string) => {
    if (
      !confirm(
        `Are you sure you want to delete "${name}"? This action cannot be undone.`
      )
    )
      return;

    try {
      await projectsApi.delete(id);
    } catch (error) {
      console.error('Failed to delete project:', error);
      setError('Failed to delete project');
    }
  };

  return (
    <Card
      className="overflow-hidden hover:shadow-md transition-shadow cursor-pointer focus:ring-2 focus:ring-primary outline-none border"
      onClick={() => navigate(`/local-projects/${project.id}/tasks`)}
      tabIndex={isFocused ? 0 : -1}
      ref={ref}
    >
      {/* Gradient header */}
      <div className={`h-24 bg-gradient-to-br ${gradientClass} opacity-80`} />

      {/* Content */}
      <div className="p-4 space-y-3">
        {/* Title + menu row */}
        <div className="flex items-start justify-between">
          <h3 className="font-semibold text-lg leading-tight truncate pr-2">
            {project.name}
          </h3>
          <DropdownMenu>
            <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 shrink-0"
              >
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                onClick={(e) => {
                  e.stopPropagation();
                  navigate(`/local-projects/${project.id}`);
                }}
              >
                <ExternalLink className="mr-2 h-4 w-4" />
                {t('viewProject')}
              </DropdownMenuItem>
              {isSingleRepoProject && (
                <DropdownMenuItem
                  onClick={(e) => {
                    e.stopPropagation();
                    handleOpenInEditor();
                  }}
                >
                  <FolderOpen className="mr-2 h-4 w-4" />
                  {t('openInIDE')}
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onClick={(e) => {
                  e.stopPropagation();
                  onEdit(project);
                }}
              >
                <Edit className="mr-2 h-4 w-4" />
                {t('common:buttons.edit')}
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(project.id, project.name);
                }}
                className="text-destructive"
              >
                <Trash2 className="mr-2 h-4 w-4" />
                {t('common:buttons.delete')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* Stats row */}
        <div className="grid grid-cols-3 gap-2">
          {stats ? (
            <>
              <div className="rounded-md bg-muted px-2.5 py-1.5 text-center">
                <div className="flex items-center justify-center gap-1 text-sm font-semibold tabular-nums">
                  <GitCommitHorizontal className="h-3 w-3 text-muted-foreground" />
                  {stats.commits_this_month}
                </div>
                <div className="text-[10px] text-muted-foreground leading-tight mt-0.5">
                  commits
                </div>
              </div>
              <div className="rounded-md bg-emerald-500/10 px-2.5 py-1.5 text-center">
                <div className="flex items-center justify-center gap-0.5 text-sm font-semibold tabular-nums text-emerald-600 dark:text-emerald-400">
                  <Plus className="h-3 w-3" />
                  {stats.lines_added_this_month.toLocaleString()}
                </div>
                <div className="text-[10px] text-muted-foreground leading-tight mt-0.5">
                  added
                </div>
              </div>
              <div className="rounded-md bg-red-500/10 px-2.5 py-1.5 text-center">
                <div className="flex items-center justify-center gap-0.5 text-sm font-semibold tabular-nums text-red-600 dark:text-red-400">
                  <Minus className="h-3 w-3" />
                  {stats.lines_removed_this_month.toLocaleString()}
                </div>
                <div className="text-[10px] text-muted-foreground leading-tight mt-0.5">
                  removed
                </div>
              </div>
            </>
          ) : (
            <>
              <div className="rounded-md bg-muted px-2.5 py-1.5 h-10 animate-pulse" />
              <div className="rounded-md bg-muted px-2.5 py-1.5 h-10 animate-pulse" />
              <div className="rounded-md bg-muted px-2.5 py-1.5 h-10 animate-pulse" />
            </>
          )}
        </div>

        {/* Last commit */}
        <p className="text-xs text-muted-foreground">
          {stats
            ? `Last commit: ${formatRelativeTime(stats.last_commit_date)}`
            : '\u00A0'}
        </p>
      </div>
    </Card>
  );
}

export default ProjectCard;
