import { TaskStatus } from 'shared/types';

export const statusLabels: Record<TaskStatus, string> = {
  backlog: 'Backlog',
  todo: 'To Do',
  spec: 'Spec',
  plan: 'Plan',
  ralph: 'Ralph',
  inreview: 'In Review',
  done: 'Done',
  cancelled: 'Cancelled',
};

export const statusBoardColors: Record<TaskStatus, string> = {
  backlog: '--neutral-foreground',
  todo: '--neutral-foreground',
  spec: '--info',
  plan: '--info',
  ralph: '--warning',
  inreview: '--warning',
  done: '--success',
  cancelled: '--destructive',
};
