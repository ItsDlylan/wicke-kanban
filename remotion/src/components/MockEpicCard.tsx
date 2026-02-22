import React from 'react';
import { colors, fonts, shadows } from '../styles/theme';

interface MockEpicCardProps {
  title: string;
  hasSpec?: boolean;
  style?: React.CSSProperties;
  opacity?: number;
  scale?: number;
}

export const MockEpicCard: React.FC<MockEpicCardProps> = ({
  title,
  hasSpec = false,
  style,
  opacity = 1,
  scale = 1,
}) => {
  return (
    <div
      style={{
        background: colors.epicPurpleBg,
        borderRadius: 8,
        borderLeft: `3px solid ${colors.epicPurple}`,
        padding: '10px 12px',
        boxShadow: shadows.card,
        fontFamily: fonts.sans,
        opacity,
        transform: `scale(${scale})`,
        position: 'relative',
        ...style,
      }}
    >
      {/* E badge */}
      <div
        style={{
          position: 'absolute',
          top: 6,
          right: 6,
          background: colors.epicPurple,
          color: '#fff',
          fontSize: 9,
          fontWeight: 700,
          width: 16,
          height: 16,
          borderRadius: 4,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          letterSpacing: 0,
        }}
      >
        E
      </div>

      {/* Title */}
      <div
        style={{
          fontSize: 12,
          fontWeight: 600,
          color: colors.textPrimary,
          marginRight: 20,
          lineHeight: 1.4,
        }}
      >
        {title}
      </div>

      {/* Badges row */}
      {hasSpec && (
        <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
          <div
            style={{
              fontSize: 10,
              fontWeight: 500,
              color: colors.epicPurple,
              background: 'rgba(168, 85, 247, 0.1)',
              padding: '2px 6px',
              borderRadius: 4,
            }}
          >
            Spec
          </div>
          <div
            style={{
              fontSize: 10,
              fontWeight: 500,
              color: colors.epicPurple,
              background: 'rgba(168, 85, 247, 0.1)',
              padding: '2px 6px',
              borderRadius: 4,
            }}
          >
            Epic
          </div>
        </div>
      )}
    </div>
  );
};
