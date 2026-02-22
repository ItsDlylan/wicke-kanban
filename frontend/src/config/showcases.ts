import { createElement } from 'react';
import { ShowcaseConfig } from '@/types/showcase';
import { PlanningShowcaseIllustration } from '@/components/showcase/PlanningShowcaseIllustration';

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
          type: 'component' as const,
          render: () => createElement(PlanningShowcaseIllustration, { step: 'overview' }),
        },
      },
      {
        titleKey: 'showcases.planningBoard.createEpic.title',
        descriptionKey: 'showcases.planningBoard.createEpic.description',
        media: {
          type: 'component' as const,
          render: () => createElement(PlanningShowcaseIllustration, { step: 'createEpic' }),
        },
      },
      {
        titleKey: 'showcases.planningBoard.planWithClaude.title',
        descriptionKey: 'showcases.planningBoard.planWithClaude.description',
        media: {
          type: 'component' as const,
          render: () => createElement(PlanningShowcaseIllustration, { step: 'planWithClaude' }),
        },
      },
      {
        titleKey: 'showcases.planningBoard.specAndDecompose.title',
        descriptionKey: 'showcases.planningBoard.specAndDecompose.description',
        media: {
          type: 'component' as const,
          render: () =>
            createElement(PlanningShowcaseIllustration, { step: 'specAndDecompose' }),
        },
      },
      {
        titleKey: 'showcases.planningBoard.ralph.title',
        descriptionKey: 'showcases.planningBoard.ralph.description',
        media: {
          type: 'component' as const,
          render: () => createElement(PlanningShowcaseIllustration, { step: 'ralph' }),
        },
      },
    ],
  } satisfies ShowcaseConfig,
} as const;
