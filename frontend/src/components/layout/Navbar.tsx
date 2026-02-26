import { Link, useLocation, useSearchParams } from 'react-router-dom';
import { useCallback, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { FolderOpen, Settings, Menu, Plus, Copy, Check } from 'lucide-react';
import { Logo } from '@/components/Logo';
import { SearchBar } from '@/components/SearchBar';
import { useSearch } from '@/contexts/SearchContext';
import { openTaskForm } from '@/lib/openTaskForm';
import { useProject } from '@/contexts/ProjectContext';
import { useOpenProjectInEditor } from '@/hooks/useOpenProjectInEditor';
import { OpenInIdeButton } from '@/components/ide/OpenInIdeButton';
import { UsageIndicator } from '@/components/ui-new/primitives/UsageIndicator';
import { useProjectRepos } from '@/hooks';

const INTERNAL_NAV = [
  { label: 'Projects', icon: FolderOpen, to: '/local-projects' },
];

const EXTERNAL_LINKS: {
  label: string;
  icon: typeof FolderOpen;
  href: string;
}[] = [];

function NavDivider() {
  return (
    <div
      className="mx-2 h-6 w-px bg-border/60"
      role="separator"
      aria-orientation="vertical"
    />
  );
}

export function Navbar() {
  const location = useLocation();
  const { projectId, project } = useProject();
  const [searchParams] = useSearchParams();
  const { query, setQuery, active, clear, registerInputRef } = useSearch();
  const handleOpenInEditor = useOpenProjectInEditor(project || null);

  const { data: repos } = useProjectRepos(projectId);
  const isSingleRepoProject = repos?.length === 1;

  const setSearchBarRef = useCallback(
    (node: HTMLInputElement | null) => {
      registerInputRef(node);
    },
    [registerInputRef]
  );

  const [copied, setCopied] = useState(false);

  const handleCopyProjectId = useCallback(async () => {
    if (!projectId) return;
    try {
      await navigator.clipboard.writeText(projectId);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.warn('Copy to clipboard failed:', err);
    }
  }, [projectId]);

  const isPlanningBoard = searchParams.get('board') === 'planning';
  const handleCreateTask = () => {
    if (projectId) {
      openTaskForm({
        mode: 'create',
        projectId,
        ...(isPlanningBoard ? { taskType: 'epic' as const } : {}),
      });
    }
  };

  const handleOpenInIDE = () => {
    handleOpenInEditor();
  };

  return (
    <div className="border-b bg-background">
      <div className="w-full px-3">
        <div className="flex items-center h-12 py-2">
          <div className="flex-1 flex items-center min-w-0">
            <Link to="/local-projects" className="shrink-0">
              <Logo />
            </Link>
            {project && (
              <>
                <span className="text-muted-foreground/60 mx-1.5">/</span>
                <span className="text-sm font-medium truncate max-w-[200px]">
                  {project.name}
                </span>
                <button
                  type="button"
                  onClick={handleCopyProjectId}
                  className="ml-1.5 flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors rounded px-1.5 py-0.5 hover:bg-muted shrink-0"
                  title="Copy Project ID"
                >
                  <code className="font-mono">{projectId?.slice(0, 8)}...</code>
                  {copied ? (
                    <Check className="h-3 w-3 text-green-500" />
                  ) : (
                    <Copy className="h-3 w-3" />
                  )}
                </button>
              </>
            )}
          </div>

          <div className="hidden sm:flex items-center gap-2">
            <SearchBar
              ref={setSearchBarRef}
              className="shrink-0"
              value={query}
              onChange={setQuery}
              disabled={!active}
              onClear={clear}
              project={project || null}
            />
          </div>

          <div className="flex flex-1 items-center justify-end gap-1">
            <UsageIndicator />
            {projectId ? (
              <>
                <div className="flex items-center gap-1">
                  {isSingleRepoProject && (
                    <OpenInIdeButton
                      onClick={handleOpenInIDE}
                      className="h-9 w-9"
                    />
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-9 w-9"
                    onClick={handleCreateTask}
                    aria-label="Create new task"
                  >
                    <Plus className="h-4 w-4" />
                  </Button>
                </div>
                <NavDivider />
              </>
            ) : null}

            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-9 w-9"
                asChild
                aria-label="Settings"
              >
                <Link
                  to={
                    projectId
                      ? `/settings/projects?projectId=${projectId}`
                      : '/settings'
                  }
                >
                  <Settings className="h-4 w-4" />
                </Link>
              </Button>

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-9 w-9"
                    aria-label="Main navigation"
                  >
                    <Menu className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>

                <DropdownMenuContent align="end">
                  {INTERNAL_NAV.map((item) => {
                    const active = location.pathname.startsWith(item.to);
                    const Icon = item.icon;
                    return (
                      <DropdownMenuItem
                        key={item.to}
                        asChild
                        className={active ? 'bg-accent' : ''}
                      >
                        <Link to={item.to}>
                          <Icon className="mr-2 h-4 w-4" />
                          {item.label}
                        </Link>
                      </DropdownMenuItem>
                    );
                  })}

                  <DropdownMenuSeparator />

                  {EXTERNAL_LINKS.map((item) => {
                    const Icon = item.icon;
                    return (
                      <DropdownMenuItem key={item.href} asChild>
                        <a
                          href={item.href}
                          target="_blank"
                          rel="noopener noreferrer"
                        >
                          <Icon className="mr-2 h-4 w-4" />
                          {item.label}
                        </a>
                      </DropdownMenuItem>
                    );
                  })}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
