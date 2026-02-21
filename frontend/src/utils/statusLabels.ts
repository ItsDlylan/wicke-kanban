import { TaskStatus } from 'shared/types';

export const statusLabels: Record<TaskStatus, string> = {
  backlog: 'Backlog',
  plangenerating: 'Plan Generating',
  ready: 'Ready',
  ralph: 'Ralph',
  inprogress: 'In Progress',
  qa: 'Q/A',
  done: 'Done',
  cancelled: 'Cancelled',
  idea: 'Idea',
  planning: 'Planning',
  specreview: 'Spec Review',
};

export const statusBoardColors: Record<TaskStatus, string> = {
  backlog: '--neutral-foreground',
  plangenerating: '--info',
  ready: '--neutral-foreground',
  ralph: '--warning',
  inprogress: '--warning',
  qa: '--warning',
  done: '--success',
  cancelled: '--destructive',
  idea: '--info',
  planning: '--warning',
  specreview: '--info',
};
