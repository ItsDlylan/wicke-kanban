import React from 'react';
import {
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
  Easing,
} from 'remotion';
import { MockKanbanBoard } from '../components/MockKanbanBoard';
import { MockEpicCard } from '../components/MockEpicCard';
import { MockTaskCard } from '../components/MockTaskCard';
import { colors, fonts, planningColumns } from '../styles/theme';

const childStories = [
  'Set up OAuth2 provider config',
  'Implement JWT token service',
  'Create login/signup UI',
];

export const RalphExecutes: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // Scale frame numbers from 30fps authoring rate to current fps
  const at = (f30: number) => Math.round((f30 * fps) / 30);

  // Phase 1 (0-1s): Board appears with parent in Ralph column
  const boardOpacity = interpolate(frame, [0, at(20)], [0, 1], {
    extrapolateRight: 'clamp',
  });

  // Each child story goes through: idle → active (pulse + bot) → progress → complete
  const storyTimings = childStories.map((_, i) => ({
    activateAt: at(40) + i * at(70),
    progressStart: at(55) + i * at(70),
    progressEnd: at(90) + i * at(70),
    completeAt: at(95) + i * at(70),
  }));

  const getStoryState = (
    i: number
  ): 'idle' | 'active' | 'progress' | 'complete' => {
    const t = storyTimings[i];
    if (frame >= t.completeAt) return 'complete';
    if (frame >= t.progressStart) return 'progress';
    if (frame >= t.activateAt) return 'active';
    return 'idle';
  };

  const getProgressWidth = (i: number) => {
    const t = storyTimings[i];
    return interpolate(frame, [t.progressStart, t.progressEnd], [0, 100], {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
      easing: Easing.inOut(Easing.cubic),
    });
  };

  const getCheckmarkScale = (i: number) => {
    const t = storyTimings[i];
    return spring({
      frame: frame - t.completeAt,
      fps,
      config: { damping: 10, stiffness: 150, mass: 0.5 },
    });
  };

  // Bot icon pulse
  const getBotPulse = (i: number) => {
    const t = storyTimings[i];
    if (frame < t.activateAt || frame >= t.completeAt) return 0;
    const pulseCycle = at(20);
    const cycle = (frame - t.activateAt) % pulseCycle;
    return interpolate(
      cycle,
      [0, pulseCycle / 2, pulseCycle],
      [0.6, 1, 0.6]
    );
  };

  // Branch line animation
  const getBranchProgress = (i: number) => {
    const t = storyTimings[i];
    return interpolate(
      frame,
      [t.activateAt, t.activateAt + at(15)],
      [0, 1],
      {
        extrapolateLeft: 'clamp',
        extrapolateRight: 'clamp',
      }
    );
  };

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        background: '#fff',
        display: 'flex',
        flexDirection: 'column',
        padding: 24,
        opacity: boardOpacity,
      }}
    >
      <MockKanbanBoard
        columns={planningColumns}
        renderColumn={(col, colIndex) => {
          // Ralph column (index 4)
          if (colIndex === 4) {
            return (
              <>
                {/* Parent epic */}
                <MockEpicCard title="User authentication flow" hasSpec />

                {/* Child stories */}
                <div
                  style={{
                    display: 'flex',
                    flexDirection: 'column',
                    gap: 6,
                    marginTop: 8,
                  }}
                >
                  {childStories.map((story, i) => {
                    const state = getStoryState(i);
                    const branchProgress = getBranchProgress(i);
                    const botPulse = getBotPulse(i);

                    return (
                      <div
                        key={i}
                        style={{ position: 'relative' }}
                      >
                        {/* Branch line */}
                        <div
                          style={{
                            position: 'absolute',
                            left: -8,
                            top: 0,
                            bottom: 0,
                            width: 2,
                            background: colors.branchLine,
                            borderRadius: 1,
                            opacity: branchProgress,
                            transform: `scaleY(${branchProgress})`,
                            transformOrigin: 'top',
                          }}
                        />

                        <div
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 6,
                          }}
                        >
                          <div style={{ flex: 1 }}>
                            <MockTaskCard
                              title={story}
                              isChild
                              completed={state === 'complete'}
                              inProgress={
                                state === 'active' || state === 'progress'
                              }
                            />
                          </div>

                          {/* Bot icon */}
                          {(state === 'active' || state === 'progress') && (
                            <div
                              style={{
                                width: 22,
                                height: 22,
                                borderRadius: 6,
                                background: colors.ralphViolet,
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center',
                                opacity: botPulse,
                                flexShrink: 0,
                              }}
                            >
                              <svg
                                width="12"
                                height="12"
                                viewBox="0 0 12 12"
                                fill="none"
                              >
                                <rect
                                  x="2"
                                  y="3"
                                  width="8"
                                  height="6"
                                  rx="2"
                                  stroke="#fff"
                                  strokeWidth="1.2"
                                />
                                <circle
                                  cx="4.5"
                                  cy="6"
                                  r="0.8"
                                  fill="#fff"
                                />
                                <circle
                                  cx="7.5"
                                  cy="6"
                                  r="0.8"
                                  fill="#fff"
                                />
                                <line
                                  x1="6"
                                  y1="1"
                                  x2="6"
                                  y2="3"
                                  stroke="#fff"
                                  strokeWidth="1.2"
                                  strokeLinecap="round"
                                />
                              </svg>
                            </div>
                          )}

                          {/* Checkmark */}
                          {state === 'complete' && (
                            <div
                              style={{
                                width: 22,
                                height: 22,
                                borderRadius: '50%',
                                background: colors.successGreen,
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center',
                                flexShrink: 0,
                                transform: `scale(${getCheckmarkScale(i)})`,
                              }}
                            >
                              <svg
                                width="12"
                                height="12"
                                viewBox="0 0 12 12"
                                fill="none"
                              >
                                <path
                                  d="M2.5 6L5 8.5L9.5 3.5"
                                  stroke="#fff"
                                  strokeWidth="1.5"
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                />
                              </svg>
                            </div>
                          )}
                        </div>

                        {/* Progress bar */}
                        {state === 'progress' && (
                          <div
                            style={{
                              height: 3,
                              borderRadius: 2,
                              background: colors.progressBg,
                              marginTop: 4,
                              overflow: 'hidden',
                            }}
                          >
                            <div
                              style={{
                                height: '100%',
                                width: `${getProgressWidth(i)}%`,
                                background: colors.ralphViolet,
                                borderRadius: 2,
                              }}
                            />
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </>
            );
          }

          // Done column (index 5) — show completed stories arriving
          if (colIndex === 5) {
            return (
              <>
                {childStories.map((story, i) => {
                  const t = storyTimings[i];
                  if (frame < t.completeAt + at(10)) return null;
                  const arriveSpring = spring({
                    frame: frame - (t.completeAt + at(10)),
                    fps,
                    config: { damping: 14, stiffness: 100, mass: 0.6 },
                  });
                  return (
                    <div
                      key={i}
                      style={{
                        opacity: interpolate(
                          frame,
                          [t.completeAt + at(10), t.completeAt + at(15)],
                          [0, 1],
                          {
                            extrapolateLeft: 'clamp',
                            extrapolateRight: 'clamp',
                          }
                        ),
                        transform: `scale(${arriveSpring})`,
                      }}
                    >
                      <MockTaskCard title={story} completed />
                    </div>
                  );
                })}
              </>
            );
          }

          return null;
        }}
      />
    </div>
  );
};
