import React from 'react';
import { colors, fonts, ColumnConfig } from '../styles/theme';

interface MockKanbanBoardProps {
  columns: ColumnConfig[];
  columnOpacities?: number[];
  columnTranslateY?: number[];
  children?: React.ReactNode;
  style?: React.CSSProperties;
  renderColumn?: (
    column: ColumnConfig,
    index: number
  ) => React.ReactNode;
}

export const MockKanbanBoard: React.FC<MockKanbanBoardProps> = ({
  columns,
  columnOpacities,
  columnTranslateY,
  style,
  renderColumn,
}) => {
  return (
    <div
      style={{
        display: 'flex',
        gap: 12,
        padding: 16,
        background: colors.boardBg,
        borderRadius: 12,
        fontFamily: fonts.sans,
        height: '100%',
        ...style,
      }}
    >
      {columns.map((col, i) => {
        const opacity = columnOpacities?.[i] ?? 1;
        const translateY = columnTranslateY?.[i] ?? 0;

        return (
          <div
            key={col.name}
            style={{
              flex: 1,
              display: 'flex',
              flexDirection: 'column',
              minWidth: 0,
              opacity,
              transform: `translateY(${translateY}px)`,
            }}
          >
            {/* Column header */}
            <div
              style={{
                padding: '10px 12px',
                borderRadius: 8,
                background: colors.columnBg,
                marginBottom: 8,
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                }}
              >
                <div
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: '50%',
                    background: col.color,
                    flexShrink: 0,
                  }}
                />
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: 600,
                    color: colors.textPrimary,
                  }}
                >
                  {col.name}
                </span>
              </div>
              <div
                style={{
                  fontSize: 10,
                  color: colors.textMuted,
                  marginTop: 4,
                  marginLeft: 16,
                }}
              >
                {col.subtitle}
              </div>
            </div>

            {/* Column body — render custom content or empty */}
            <div
              style={{
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
                padding: '4px 0',
              }}
            >
              {renderColumn?.(col, i)}
            </div>
          </div>
        );
      })}
    </div>
  );
};
