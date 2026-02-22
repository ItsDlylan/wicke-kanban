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
import { colors, fonts, shadows, planningColumns } from '../styles/theme';

const epicTitle = 'User authentication flow';

export const CreateEpic: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // Scale frame numbers from 30fps authoring rate to current fps
  const at = (f30: number) => Math.round((f30 * fps) / 30);

  // Phase 1 (0-1s): Show board with empty Idea column highlighted
  const boardOpacity = interpolate(frame, [0, at(20)], [0, 1], {
    extrapolateRight: 'clamp',
  });

  // Phase 2 (1-2.7s): Highlight "+" button area in Idea column
  const plusGlow = interpolate(
    frame,
    [at(30), at(50), at(65), at(80)],
    [0, 1, 1, 0],
    {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
    }
  );

  // Phase 3 (2.7-6s): Dialog slides in, title types in
  const dialogProgress = interpolate(frame, [at(80), at(100)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.out(Easing.cubic),
  });

  const typingStart = at(110);
  const typingEnd = at(165);
  const typedChars = Math.floor(
    interpolate(frame, [typingStart, typingEnd], [0, epicTitle.length], {
      extrapolateLeft: 'clamp',
      extrapolateRight: 'clamp',
    })
  );
  const displayedTitle = epicTitle.slice(0, typedChars);
  const showCursor = frame >= typingStart && frame <= typingEnd + at(10);

  // Phase 4 (6-7.3s): Dialog closes
  const dialogClose = interpolate(frame, [at(185), at(200)], [1, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
    easing: Easing.in(Easing.cubic),
  });

  const dialogVisible = frame >= at(80) && frame <= at(200);

  // Phase 5 (6.7-10s): New card springs into Idea column
  const cardSpring = spring({
    frame: frame - at(210),
    fps,
    config: { damping: 10, stiffness: 120, mass: 0.6 },
  });
  const cardOpacity = interpolate(frame, [at(210), at(215)], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
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
        position: 'relative',
        opacity: boardOpacity,
      }}
    >
      {/* Board */}
      <div style={{ flex: 1 }}>
        <MockKanbanBoard
          columns={planningColumns}
          renderColumn={(col, colIndex) => {
            if (colIndex === 0) {
              return (
                <>
                  {/* + button */}
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'center',
                      padding: '8px 0',
                    }}
                  >
                    <div
                      style={{
                        width: 32,
                        height: 32,
                        borderRadius: 8,
                        border: `1.5px dashed ${colors.cardBorder}`,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        color: colors.textMuted,
                        fontSize: 18,
                        fontWeight: 300,
                        boxShadow:
                          plusGlow > 0
                            ? `0 0 ${12 * plusGlow}px rgba(168, 85, 247, ${0.3 * plusGlow})`
                            : 'none',
                      }}
                    >
                      +
                    </div>
                  </div>

                  {/* New card appears */}
                  {frame > at(210) && (
                    <MockEpicCard
                      title={epicTitle}
                      opacity={cardOpacity}
                      scale={cardSpring}
                    />
                  )}
                </>
              );
            }
            return null;
          }}
        />
      </div>

      {/* Dialog overlay */}
      {dialogVisible && (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            background: `rgba(0, 0, 0, ${0.4 * (frame < at(185) ? dialogProgress : dialogClose)})`,
            zIndex: 10,
          }}
        >
          <div
            style={{
              background: colors.dialogBg,
              borderRadius: 16,
              padding: 24,
              width: 380,
              boxShadow: shadows.dialog,
              fontFamily: fonts.sans,
              transform: `scale(${frame < at(185) ? dialogProgress : dialogClose}) translateY(${(1 - (frame < at(185) ? dialogProgress : dialogClose)) * 20}px)`,
              opacity: frame < at(185) ? dialogProgress : dialogClose,
            }}
          >
            {/* Dialog header */}
            <div
              style={{
                fontSize: 16,
                fontWeight: 600,
                color: colors.textPrimary,
                marginBottom: 16,
              }}
            >
              Create Epic
            </div>

            {/* Title field */}
            <div style={{ marginBottom: 12 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 500,
                  color: colors.textMuted,
                  marginBottom: 6,
                }}
              >
                Title
              </div>
              <div
                style={{
                  border: `1px solid ${frame >= typingStart ? colors.epicPurple : colors.inputBorder}`,
                  borderRadius: 8,
                  padding: '10px 12px',
                  fontSize: 13,
                  color: colors.textPrimary,
                  minHeight: 20,
                  display: 'flex',
                  alignItems: 'center',
                  background: colors.inputBg,
                }}
              >
                {displayedTitle}
                {showCursor && (
                  <span
                    style={{
                      display: 'inline-block',
                      width: 1.5,
                      height: 16,
                      background: colors.cursorBlue,
                      marginLeft: 1,
                    }}
                  />
                )}
              </div>
            </div>

            {/* Description field */}
            <div style={{ marginBottom: 20 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 500,
                  color: colors.textMuted,
                  marginBottom: 6,
                }}
              >
                Description (optional)
              </div>
              <div
                style={{
                  border: `1px solid ${colors.inputBorder}`,
                  borderRadius: 8,
                  padding: '10px 12px',
                  fontSize: 13,
                  color: colors.textLight,
                  height: 60,
                  background: colors.inputBg,
                }}
              >
                Describe your epic...
              </div>
            </div>

            {/* Buttons */}
            <div
              style={{
                display: 'flex',
                justifyContent: 'flex-end',
                gap: 8,
              }}
            >
              <div
                style={{
                  padding: '8px 16px',
                  borderRadius: 8,
                  fontSize: 13,
                  fontWeight: 500,
                  color: colors.textMuted,
                  background: '#f3f4f6',
                }}
              >
                Cancel
              </div>
              <div
                style={{
                  padding: '8px 16px',
                  borderRadius: 8,
                  fontSize: 13,
                  fontWeight: 600,
                  color: '#fff',
                  background:
                    typedChars > 0
                      ? colors.buttonPrimary
                      : colors.progressBg,
                }}
              >
                Create
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
