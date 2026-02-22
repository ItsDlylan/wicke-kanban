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
import { MockSpecSheet } from '../components/MockSpecSheet';
import { colors, fonts, shadows, planningColumns } from '../styles/theme';

const specFields = [
  { label: 'Objective', value: 'Implement secure user authentication with OAuth2 and JWT tokens' },
  { label: 'Acceptance Criteria', value: 'Users can sign in via Google/GitHub, sessions persist across refreshes' },
  { label: 'Technical Approach', value: 'Use passport.js for OAuth, issue JWTs with 24h expiry, refresh token rotation' },
  { label: 'Dependencies', value: 'Database user table, Redis for session cache, OAuth app credentials' },
];

const childStories = [
  'Set up OAuth2 provider config',
  'Implement JWT token service',
  'Create login/signup UI components',
  'Add session persistence middleware',
];

export const SpecDecompose: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // Scale frame numbers from 30fps authoring rate to current fps
  const at = (f30: number) => Math.round((f30 * fps) / 30);

  // Phase 1 (0-1s): Board with epic in Ready column, "Make Ralph Ready" button
  const boardOpacity = interpolate(frame, [0, at(20)], [0, 1], {
    extrapolateRight: 'clamp',
  });

  // Phase 2 (1-2.2s): Button glows
  const buttonGlow = interpolate(
    frame,
    [at(30), at(45), at(55), at(65)],
    [0, 1, 1, 0.3],
    { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }
  );

  // Phase 3 (2.2-3.7s): Spec sheet slides in
  const specSlide = interpolate(frame, [at(65), at(95)], [300, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });
  const specOpacity = interpolate(frame, [at(65), at(85)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  // Phase 4 (3.3-6.7s): Fields fill in one by one with generation spinner
  const fieldTimings = specFields.map((_, i) => ({
    generatingStart: at(100) + i * at(25),
    generatingEnd: at(115) + i * at(25),
    visibleAt: at(115) + i * at(25),
  }));

  const visibleFields = fieldTimings.filter(
    (t) => frame >= t.visibleAt
  ).length;

  const generatingField = fieldTimings.findIndex(
    (t) => frame >= t.generatingStart && frame < t.generatingEnd
  );

  // Phase 5 (7-10s): Child stories cascade out
  const childCards = childStories.map((_, i) => {
    const delay = at(215) + i * at(18);
    const s = spring({
      frame: frame - delay,
      fps,
      config: { damping: 12, stiffness: 100, mass: 0.6 },
    });
    return {
      opacity: interpolate(frame, [delay, delay + at(8)], [0, 1], {
        extrapolateLeft: 'clamp',
        extrapolateRight: 'clamp',
      }),
      scale: s,
      translateX: interpolate(frame, [delay, delay + at(15)], [-20, 0], {
        extrapolateLeft: 'clamp',
        extrapolateRight: 'clamp',
        easing: Easing.out(Easing.cubic),
      }),
    };
  });

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
            // Ready column (index 3)
            if (colIndex === 3) {
              return (
                <>
                  <MockEpicCard title="User authentication flow" hasSpec />

                  {/* Make Ralph Ready button */}
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'center',
                      marginTop: 8,
                    }}
                  >
                    <div
                      style={{
                        padding: '6px 14px',
                        borderRadius: 8,
                        fontSize: 11,
                        fontWeight: 600,
                        color: '#fff',
                        background: colors.buttonPrimary,
                        fontFamily: fonts.sans,
                        boxShadow:
                          buttonGlow > 0
                            ? `0 0 ${16 * buttonGlow}px rgba(124, 58, 237, ${0.4 * buttonGlow})`
                            : 'none',
                      }}
                    >
                      Make Ralph Ready
                    </div>
                  </div>

                  {/* Child stories */}
                  {frame >= at(215) && (
                    <div
                      style={{
                        marginTop: 12,
                        display: 'flex',
                        flexDirection: 'column',
                        gap: 6,
                      }}
                    >
                      {childStories.map((story, i) => (
                        <div
                          key={i}
                          style={{
                            opacity: childCards[i].opacity,
                            transform: `scale(${childCards[i].scale}) translateX(${childCards[i].translateX}px)`,
                          }}
                        >
                          <MockTaskCard title={story} isChild />
                        </div>
                      ))}
                    </div>
                  )}
                </>
              );
            }
            return null;
          }}
        />
      </div>

      {/* Spec sheet panel */}
      {frame >= at(65) && (
        <div
          style={{
            transform: `translateX(${specSlide}px)`,
            opacity: specOpacity,
            flexShrink: 0,
          }}
        >
          <MockSpecSheet
            fields={specFields}
            visibleFields={visibleFields}
            generatingField={generatingField}
          />
        </div>
      )}
    </div>
  );
};
