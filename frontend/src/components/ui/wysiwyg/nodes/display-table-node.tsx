import { NodeKey } from 'lexical';
import {
  createDecoratorNode,
  type DecoratorNodeConfig,
} from '../lib/create-decorator-node';

interface DisplayTableData {
  headers: string[];
  rows: string[][];
}

function DisplayTableComponent({
  data,
}: {
  data: DisplayTableData;
  nodeKey: NodeKey;
  onDoubleClickEdit: (event: React.MouseEvent) => void;
}) {
  const { headers, rows } = data;
  const colCount = headers.length;

  return (
    <div className="overflow-x-auto my-2">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            {headers.map((header, i) => (
              <th
                key={i}
                className="bg-muted font-semibold border border-border px-3 py-2 text-left text-sm"
              >
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, ri) => (
            <tr key={ri}>
              {Array.from({ length: colCount }, (_, ci) => (
                <td key={ci} className="border border-border px-3 py-2 text-sm">
                  {row[ci] ?? ''}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const config: DecoratorNodeConfig<DisplayTableData> = {
  type: 'display-table',
  serialization: {
    format: 'fenced',
    language: 'display-table',
    serialize: (data) => JSON.stringify(data, null, 2),
    deserialize: (content) => JSON.parse(content),
    validate: (data) => Array.isArray(data.headers) && Array.isArray(data.rows),
  },
  component: DisplayTableComponent,
};

const result = createDecoratorNode(config);

export const DisplayTableNode = result.Node;
export const $createDisplayTableNode = result.createNode;
export const $isDisplayTableNode = result.isNode;
export const [DISPLAY_TABLE_EXPORT_TRANSFORMER, DISPLAY_TABLE_TRANSFORMER] =
  result.transformers;
