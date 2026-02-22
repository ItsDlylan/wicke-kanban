import React from 'react';
import { colors, fonts } from '../styles/theme';

interface MockBoardToggleProps {
  activeTab: 'tasks' | 'planning';
  style?: React.CSSProperties;
}

export const MockBoardToggle: React.FC<MockBoardToggleProps> = ({
  activeTab,
  style,
}) => {
  const pill = (label: string, count: number, active: boolean) => (
    <div
      style={{
        padding: '6px 14px',
        borderRadius: 6,
        fontSize: 13,
        fontWeight: active ? 600 : 400,
        color: active ? colors.textPrimary : colors.textMuted,
        background: active ? colors.toggleActiveBg : colors.toggleInactiveBg,
        fontFamily: fonts.sans,
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        cursor: 'default',
      }}
    >
      {label}
      <span
        style={{
          fontSize: 11,
          color: colors.textLight,
          fontWeight: 400,
        }}
      >
        ({count})
      </span>
    </div>
  );

  return (
    <div
      style={{
        display: 'inline-flex',
        gap: 2,
        padding: 3,
        borderRadius: 8,
        background: '#f9fafb',
        border: `1px solid ${colors.cardBorder}`,
        ...style,
      }}
    >
      {pill('Tasks', 12, activeTab === 'tasks')}
      {pill('Planning', 6, activeTab === 'planning')}
    </div>
  );
};
