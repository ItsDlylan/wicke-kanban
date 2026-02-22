export const colors = {
  // Epic purple
  epicPurple: '#a855f7',
  epicPurpleBg: '#faf5ff',
  epicPurpleBadge: '#9333ea',

  // Column status colors
  ideaAmber: '#f59e0b',
  planningBlue: '#3b82f6',
  specBlue: '#3b82f6',
  readyNeutral: '#6b7280',
  ralphViolet: '#8b5cf6',
  doneGreen: '#22c55e',

  // Card
  cardBg: '#ffffff',
  cardBorder: '#e5e7eb',

  // Column
  columnBg: '#f9fafb',
  columnHeaderBg: '#f3f4f6',

  // Text
  textPrimary: '#111827',
  textSecondary: '#374151',
  textMuted: '#6b7280',
  textLight: '#9ca3af',

  // Board
  boardBg: '#ffffff',

  // Toggle
  toggleActiveBg: '#f3f4f6',
  toggleInactiveBg: 'transparent',

  // Misc
  buttonPrimary: '#7c3aed',
  buttonPrimaryHover: '#6d28d9',
  successGreen: '#22c55e',
  progressBg: '#e5e7eb',
  dialogOverlay: 'rgba(0, 0, 0, 0.4)',
  dialogBg: '#ffffff',
  inputBorder: '#d1d5db',
  inputBg: '#ffffff',
  cursorBlue: '#3b82f6',
  spinnerPurple: '#a855f7',
  branchLine: '#8b5cf6',
} as const;

export const fonts = {
  sans: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
  mono: '"SF Mono", "Fira Code", "Fira Mono", "Roboto Mono", monospace',
} as const;

export const shadows = {
  card: '0 1px 3px rgba(0, 0, 0, 0.08), 0 1px 2px rgba(0, 0, 0, 0.06)',
  cardHover: '0 4px 12px rgba(0, 0, 0, 0.1), 0 2px 4px rgba(0, 0, 0, 0.06)',
  dialog: '0 20px 60px rgba(0, 0, 0, 0.15), 0 8px 20px rgba(0, 0, 0, 0.1)',
  panel: '0 4px 16px rgba(0, 0, 0, 0.08)',
} as const;

export type ColumnConfig = {
  name: string;
  color: string;
  subtitle: string;
};

export const planningColumns: ColumnConfig[] = [
  { name: 'Idea', color: colors.ideaAmber, subtitle: 'New concepts' },
  { name: 'Planning', color: colors.planningBlue, subtitle: 'Scoping with AI' },
  { name: 'Spec Review', color: colors.specBlue, subtitle: 'Review specs' },
  { name: 'Ready', color: colors.readyNeutral, subtitle: 'Approved' },
  { name: 'Ralph', color: colors.ralphViolet, subtitle: 'AI executing' },
  { name: 'Done', color: colors.doneGreen, subtitle: 'Completed' },
];
