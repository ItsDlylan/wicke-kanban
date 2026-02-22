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
import { MockSessionPanel } from '../components/MockSessionPanel';
import { colors, planningColumns } from '../styles/theme';

const referenceText = 'Discussed auth provider options, settled on OAuth2 + JWT combo';

export const PlanWithClaude: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // Scale frame numbers from 30fps authoring rate to current fps
  const at = (f30: number) => Math.round((f30 * fps) / 30);

  // Phase 1 (0-40): Board appears with epic in Planning column
  const boardOpacity = interpolate(frame, [0, at(20)], [0, 1], {
    extrapolateRight: 'clamp',
  });

  // Phase 2 (40-90): Session panel slides in from right
  const panelSlide = interpolate(frame, [at(40), at(75)], [300, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const panelOpacity = interpolate(frame, [at(40), at(60)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  // Phase 3 (90-180): Reference text types in
  const typingProgress = interpolate(frame, [at(90), at(175)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  // Phase 4 (180-220): New session card appears
  const sessionCardSpring = spring({
    frame: frame - at(185),
    fps,
    config: { damping: 12, stiffness: 100, mass: 0.8 },
  });
  const sessionCardOpacity = interpolate(
    frame,
    [at(185), at(195)],
    [0, 1],
    {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
    }
  );

  // Phase 5 (230-300): Epic moves from Planning → Spec Review
  const moveProgress = interpolate(frame, [at(235), at(275)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.inOut(Easing.cubic),
  });

  const sessions =
    frame >= at(185)
      ? [
          { title: 'Auth architecture discussion', date: 'Just now' },
          { title: 'Initial scoping session', date: '2 days ago' },
        ]
      : [{ title: 'Initial scoping session', date: '2 days ago' }];

  // For the moving card, compute column positions
  const showInPlanning = frame < at(275);
  const showInSpecReview = frame >= at(250);

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        background: '#fff',
        display: 'flex',
        padding: 24,
        gap: 16,
        opacity: boardOpacity,
      }}
    >
      {/* Board */}
      <div style={{ flex: 1 }}>
        <MockKanbanBoard
          columns={planningColumns}
          renderColumn={(col, colIndex) => {
            // Planning column (index 1)
            if (colIndex === 1 && showInPlanning) {
              return (
                <MockEpicCard
                  title="User authentication flow"
                  opacity={interpolate(
                    moveProgress,
                    [0, 0.3],
                    [1, 0],
                    { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }
                  )}
                  style={{
                    transform: `translateX(${moveProgress * 40}px)`,
                  }}
                />
              );
            }
            // Spec Review column (index 2)
            if (colIndex === 2 && showInSpecReview) {
              return (
                <MockEpicCard
                  title="User authentication flow"
                  hasSpec
                  opacity={interpolate(
                    moveProgress,
                    [0.5, 0.8],
                    [0, 1],
                    { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }
                  )}
                  scale={spring({
                    frame: frame - at(260),
                    fps,
                    config: { damping: 12, stiffness: 80, mass: 0.8 },
                  })}
                />
              );
            }
            return null;
          }}
        />
      </div>

      {/* Session panel */}
      <div
        style={{
          transform: `translateX(${panelSlide}px)`,
          opacity: panelOpacity,
          flexShrink: 0,
        }}
      >
        <MockSessionPanel
          sessions={sessions}
          referenceText={referenceText}
          typingProgress={typingProgress}
        />
        {/* New session card spring animation */}
        {frame >= at(185) && frame < at(195) && (
          <div
            style={{
              position: 'absolute',
              opacity: sessionCardOpacity,
              transform: `scale(${sessionCardSpring})`,
            }}
          />
        )}
      </div>
    </div>
  );
};
