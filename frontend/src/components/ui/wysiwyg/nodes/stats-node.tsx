import { NodeKey } from 'lexical';
import { TrendingUp, TrendingDown, Minus } from 'lucide-react';
import {
  createDecoratorNode,
  type DecoratorNodeConfig,
} from '../lib/create-decorator-node';

interface StatsItem {
  label: string;
  value: string;
  trend?: 'up' | 'down' | 'neutral';
}

interface StatsData {
  items: StatsItem[];
}

const MAX_ITEMS = 12;

function StatsComponent({
  data,
}: {
  data: StatsData;
  nodeKey: NodeKey;
  onDoubleClickEdit: (event: React.MouseEvent) => void;
}) {
  const items = data.items.slice(0, MAX_ITEMS);

  return (
    <div className="grid grid-cols-2 gap-3 my-2">
      {items.map((item, i) => (
        <div key={i} className="rounded-md border border-border p-3">
          <div className="text-xs text-muted-foreground">{item.label}</div>
          <div className="flex items-center gap-1.5 mt-1">
            <span className="text-lg font-semibold">{item.value}</span>
            {item.trend === 'up' && (
              <TrendingUp className="h-4 w-4 text-green-500" />
            )}
            {item.trend === 'down' && (
              <TrendingDown className="h-4 w-4 text-red-500" />
            )}
            {item.trend === 'neutral' && (
              <Minus className="h-4 w-4 text-muted-foreground" />
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

const config: DecoratorNodeConfig<StatsData> = {
  type: 'stats',
  serialization: {
    format: 'fenced',
    language: 'stats',
    serialize: (data) => JSON.stringify(data, null, 2),
    deserialize: (content) => JSON.parse(content),
    validate: (data) => Array.isArray(data.items) && data.items.length > 0,
  },
  component: StatsComponent,
};

const result = createDecoratorNode(config);

export const StatsNode = result.Node;
export const $createStatsNode = result.createNode;
export const $isStatsNode = result.isNode;
export const [STATS_EXPORT_TRANSFORMER, STATS_TRANSFORMER] =
  result.transformers;
