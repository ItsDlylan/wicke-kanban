import { ShowcaseConfig } from '@/types/showcase';

const PLANNING_VIDEO_BASE = '/showcase/planning-board';

export const showcases = {
  taskPanel: {
    id: 'task-panel-onboarding',
    stages: [
      {
        titleKey: 'showcases.taskPanel.companion.title',
        descriptionKey: 'showcases.taskPanel.companion.description',
        media: {
          type: 'video',
          src: 'https://vkcdn.britannio.dev/showcase/flat-task-panel/vk-onb-companion-demo-3.mp4',
        },
      },
      {
        titleKey: 'showcases.taskPanel.installation.title',
        descriptionKey: 'showcases.taskPanel.installation.description',
        media: {
          type: 'video',
          src: 'https://vkcdn.britannio.dev/showcase/flat-task-panel/vk-onb-install-companion-3.mp4',
        },
      },
      {
        titleKey: 'showcases.taskPanel.codeReview.title',
        descriptionKey: 'showcases.taskPanel.codeReview.description',
        media: {
          type: 'video',
          src: 'https://vkcdn.britannio.dev/showcase/flat-task-panel/vk-onb-code-review-3.mp4',
        },
      },
      {
        titleKey: 'showcases.taskPanel.pullRequest.title',
        descriptionKey: 'showcases.taskPanel.pullRequest.description',
        media: {
          type: 'video',
          src: 'https://vkcdn.britannio.dev/showcase/flat-task-panel/vk-onb-git-pr-3.mp4',
        },
      },
      {
        titleKey: 'showcases.taskPanel.tags.title',
        descriptionKey: 'showcases.taskPanel.tags.description',
        media: {
          type: 'video',
          src: 'https://vkcdn.britannio.dev/showcase/flat-task-panel/vk-tags.mp4',
        },
      },
    ],
  } satisfies ShowcaseConfig,

  planningBoard: {
    id: 'planning-board-onboarding',
    stages: [
      {
        titleKey: 'showcases.planningBoard.overview.title',
        descriptionKey: 'showcases.planningBoard.overview.description',
        media: {
          type: 'video' as const,
          src: `${PLANNING_VIDEO_BASE}/overview.mp4`,
        },
      },
      {
        titleKey: 'showcases.planningBoard.createEpic.title',
        descriptionKey: 'showcases.planningBoard.createEpic.description',
        media: {
          type: 'video' as const,
          src: `${PLANNING_VIDEO_BASE}/create-epic.mp4`,
        },
      },
      {
        titleKey: 'showcases.planningBoard.planWithClaude.title',
        descriptionKey: 'showcases.planningBoard.planWithClaude.description',
        media: {
          type: 'video' as const,
          src: `${PLANNING_VIDEO_BASE}/plan-with-claude.mp4`,
        },
      },
      {
        titleKey: 'showcases.planningBoard.specAndDecompose.title',
        descriptionKey: 'showcases.planningBoard.specAndDecompose.description',
        media: {
          type: 'video' as const,
          src: `${PLANNING_VIDEO_BASE}/spec-decompose.mp4`,
        },
      },
      {
        titleKey: 'showcases.planningBoard.ralph.title',
        descriptionKey: 'showcases.planningBoard.ralph.description',
        media: {
          type: 'video' as const,
          src: `${PLANNING_VIDEO_BASE}/ralph-executes.mp4`,
        },
      },
    ],
  } satisfies ShowcaseConfig,
} as const;
