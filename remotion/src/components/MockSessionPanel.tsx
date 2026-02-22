import React from 'react';
import { colors, fonts, shadows } from '../styles/theme';

interface SessionItem {
  title: string;
  date: string;
}

interface MockSessionPanelProps {
  sessions: SessionItem[];
  referenceText?: string;
  typingProgress?: number;
  style?: React.CSSProperties;
  opacity?: number;
}

export const MockSessionPanel: React.FC<MockSessionPanelProps> = ({
  sessions,
  referenceText = '',
  typingProgress = 0,
  style,
  opacity = 1,
}) => {
  const displayedText = referenceText.slice(
    0,
    Math.floor(referenceText.length * typingProgress)
  );

  return (
    <div
      style={{
        background: colors.cardBg,
        borderRadius: 12,
        border: `1px solid ${colors.cardBorder}`,
        boxShadow: shadows.panel,
        padding: 16,
        fontFamily: fonts.sans,
        width: 280,
        opacity,
        ...style,
      }}
    >
      {/* Header */}
      <div
        style={{
          fontSize: 13,
          fontWeight: 600,
          color: colors.textPrimary,
          marginBottom: 12,
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path
            d="M12 4.5L5.5 11L2 7.5"
            stroke={colors.planningBlue}
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
        Planning Sessions
      </div>

      {/* Reference input */}
      {referenceText && (
        <div
          style={{
            background: '#f9fafb',
            border: `1px solid ${colors.inputBorder}`,
            borderRadius: 6,
            padding: '8px 10px',
            fontSize: 11,
            color: colors.textSecondary,
            marginBottom: 12,
            minHeight: 28,
            display: 'flex',
            alignItems: 'center',
          }}
        >
          {displayedText}
          {typingProgress < 1 && typingProgress > 0 && (
            <span
              style={{
                display: 'inline-block',
                width: 1,
                height: 14,
                background: colors.cursorBlue,
                marginLeft: 1,
                animation: 'blink 1s step-end infinite',
              }}
            />
          )}
        </div>
      )}

      {/* Session cards */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {sessions.map((session, i) => (
          <div
            key={i}
            style={{
              background: '#f9fafb',
              borderRadius: 8,
              padding: '10px 12px',
              border: `1px solid ${colors.cardBorder}`,
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 500,
                color: colors.textPrimary,
                marginBottom: 4,
              }}
            >
              {session.title}
            </div>
            <div
              style={{
                fontSize: 10,
                color: colors.textMuted,
              }}
            >
              {session.date}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
