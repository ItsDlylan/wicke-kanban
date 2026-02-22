import React from 'react';
import { colors, fonts, shadows } from '../styles/theme';

interface MockTaskCardProps {
  title: string;
  isChild?: boolean;
  completed?: boolean;
  inProgress?: boolean;
  style?: React.CSSProperties;
  opacity?: number;
}

export const MockTaskCard: React.FC<MockTaskCardProps> = ({
  title,
  isChild = false,
  completed = false,
  inProgress = false,
  style,
  opacity = 1,
}) => {
  return (
    <div
      style={{
        background: colors.cardBg,
        borderRadius: 8,
        border: `1px solid ${colors.cardBorder}`,
        padding: '10px 12px',
        boxShadow: shadows.card,
        fontFamily: fonts.sans,
        opacity,
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        ...style,
      }}
    >
      {/* Status indicator */}
      {(completed || inProgress) && (
        <div
          style={{
            width: 16,
            height: 16,
            borderRadius: '50%',
            flexShrink: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            ...(completed
              ? { background: colors.successGreen }
              : { border: `2px solid ${colors.ralphViolet}` }),
          }}
        >
          {completed && (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <path
                d="M2 5L4.5 7.5L8 3"
                stroke="#fff"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          )}
          {inProgress && (
            <div
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: colors.ralphViolet,
              }}
            />
          )}
        </div>
      )}

      {/* Indent for child */}
      {isChild && !completed && !inProgress && (
        <div
          style={{
            width: 2,
            height: 16,
            background: colors.epicPurple,
            borderRadius: 1,
            flexShrink: 0,
            marginLeft: 2,
          }}
        />
      )}

      <div
        style={{
          fontSize: 12,
          fontWeight: 500,
          color: completed ? colors.textMuted : colors.textPrimary,
          textDecoration: completed ? 'line-through' : 'none',
          lineHeight: 1.4,
        }}
      >
        {title}
      </div>
    </div>
  );
};
