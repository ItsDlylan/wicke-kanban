import React from 'react';
import { colors, fonts, shadows } from '../styles/theme';

interface SpecField {
  label: string;
  value: string;
}

interface MockSpecSheetProps {
  fields: SpecField[];
  visibleFields?: number;
  generatingField?: number;
  style?: React.CSSProperties;
  opacity?: number;
}

export const MockSpecSheet: React.FC<MockSpecSheetProps> = ({
  fields,
  visibleFields = fields.length,
  generatingField = -1,
  style,
  opacity = 1,
}) => {
  return (
    <div
      style={{
        background: colors.cardBg,
        borderRadius: 12,
        border: `1px solid ${colors.cardBorder}`,
        boxShadow: shadows.panel,
        padding: 20,
        fontFamily: fonts.sans,
        width: 320,
        opacity,
        ...style,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          marginBottom: 16,
        }}
      >
        <div
          style={{
            width: 24,
            height: 24,
            borderRadius: 6,
            background: colors.epicPurpleBg,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <rect
              x="2"
              y="1"
              width="10"
              height="12"
              rx="1.5"
              stroke={colors.epicPurple}
              strokeWidth="1.2"
            />
            <line
              x1="4.5"
              y1="4"
              x2="9.5"
              y2="4"
              stroke={colors.epicPurple}
              strokeWidth="1"
              strokeLinecap="round"
            />
            <line
              x1="4.5"
              y1="6.5"
              x2="9.5"
              y2="6.5"
              stroke={colors.epicPurple}
              strokeWidth="1"
              strokeLinecap="round"
            />
            <line
              x1="4.5"
              y1="9"
              x2="7.5"
              y2="9"
              stroke={colors.epicPurple}
              strokeWidth="1"
              strokeLinecap="round"
            />
          </svg>
        </div>
        <div
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: colors.textPrimary,
          }}
        >
          Spec Sheet
        </div>
      </div>

      {/* Fields */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        {fields.slice(0, visibleFields).map((field, i) => (
          <div key={i}>
            <div
              style={{
                fontSize: 10,
                fontWeight: 600,
                color: colors.textMuted,
                textTransform: 'uppercase',
                letterSpacing: 0.5,
                marginBottom: 4,
              }}
            >
              {field.label}
            </div>
            <div
              style={{
                fontSize: 12,
                color: colors.textSecondary,
                lineHeight: 1.5,
                background: '#f9fafb',
                borderRadius: 6,
                padding: '8px 10px',
                border: `1px solid ${colors.cardBorder}`,
                minHeight: 20,
                position: 'relative',
              }}
            >
              {i === generatingField ? (
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <Spinner />
                  <span style={{ color: colors.textMuted, fontStyle: 'italic' }}>
                    Generating...
                  </span>
                </div>
              ) : (
                field.value
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const Spinner: React.FC = () => (
  <div
    style={{
      width: 12,
      height: 12,
      border: `2px solid ${colors.progressBg}`,
      borderTopColor: colors.spinnerPurple,
      borderRadius: '50%',
      animation: 'spin 0.8s linear infinite',
    }}
  />
);
