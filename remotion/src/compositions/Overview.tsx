import React from 'react';
import {
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
  Easing,
} from 'remotion';
import { MockKanbanBoard } from '../components/MockKanbanBoard';
import { MockBoardToggle } from '../components/MockBoardToggle';
import { MockEpicCard } from '../components/MockEpicCard';
import { planningColumns } from '../styles/theme';

const sampleEpics: { title: string; column: number; hasSpec?: boolean }[] = [
  { title: 'User authentication flow', column: 0 },
  { title: 'Dashboard analytics', column: 1 },
  { title: 'Notification system', column: 2, hasSpec: true },
  { title: 'API rate limiting', column: 3, hasSpec: true },
  { title: 'Dark mode support', column: 4 },
  { title: 'Onboarding wizard', column: 5, hasSpec: true },
];

export const Overview: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // Scale frame numbers from 30fps authoring rate to current fps
  const at = (f30: number) => Math.round((f30 * fps) / 30);

  // Phase 1 (0-1s): Toggle appears and switches to Planning
  const toggleOpacity = interpolate(frame, [0, at(15)], [0, 1], {
    extrapolateRight: 'clamp',
  });

  const activeTab: 'tasks' | 'planning' =
    frame > at(25) ? 'planning' : 'tasks';

  // Phase 2 (1-4s): Columns slide in left-to-right with stagger
  const columnOpacities = planningColumns.map((_, i) => {
    const delay = at(30) + i * at(10);
    return interpolate(frame, [delay, delay + at(20)], [0, 1], {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
    });
  });

  const columnTranslateY = planningColumns.map((_, i) => {
    const delay = at(30) + i * at(10);
    return interpolate(frame, [delay, delay + at(20)], [30, 0], {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
      easing: Easing.out(Easing.cubic),
    });
  });

  // Phase 3 (4-8s): Epic cards spring into columns
  const cardAppearances = sampleEpics.map((_, i) => {
    const delay = at(120) + i * at(15);
    const s = spring({
      frame: frame - delay,
      fps,
      config: { damping: 12, stiffness: 100, mass: 0.8 },
    });
    return {
      opacity: interpolate(frame, [delay, delay + at(5)], [0, 1], {
        extrapolateLeft: 'clamp',
        extrapolateRight: 'clamp',
      }),
      scale: s,
    };
  });

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        background: '#fff',
        display: 'flex',
        flexDirection: 'column',
        padding: 24,
      }}
    >
      {/* Toggle row */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          marginBottom: 16,
          opacity: toggleOpacity,
        }}
      >
        <MockBoardToggle activeTab={activeTab} />
      </div>

      {/* Board */}
      <div style={{ flex: 1 }}>
        <MockKanbanBoard
          columns={planningColumns}
          columnOpacities={columnOpacities}
          columnTranslateY={columnTranslateY}
          renderColumn={(col, colIndex) => (
            <>
              {sampleEpics
                .filter((e) => e.column === colIndex)
                .map((epic, cardIdx) => {
                  const globalIdx = sampleEpics.indexOf(epic);
                  const appearance = cardAppearances[globalIdx];
                  return (
                    <MockEpicCard
                      key={cardIdx}
                      title={epic.title}
                      hasSpec={epic.hasSpec}
                      opacity={appearance.opacity}
                      scale={appearance.scale}
                    />
                  );
                })}
            </>
          )}
        />
      </div>
    </div>
  );
};
