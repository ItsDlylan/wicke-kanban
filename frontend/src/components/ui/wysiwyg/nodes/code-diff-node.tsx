import { NodeKey } from 'lexical';
import { useMemo } from 'react';
import { DiffModeEnum, DiffView } from '@git-diff-view/react';
import { generateDiffFile } from '@git-diff-view/file';
import {
  createDecoratorNode,
  type DecoratorNodeConfig,
} from '../lib/create-decorator-node';
import '@/styles/diff-style-overrides.css';

interface CodeDiffData {
  before: string;
  after: string;
  language?: string;
  fileName?: string;
}

const MAX_COMBINED_SIZE = 500_000;

function CodeDiffComponent({
  data,
}: {
  data: CodeDiffData;
  nodeKey: NodeKey;
  onDoubleClickEdit: (event: React.MouseEvent) => void;
}) {
  const { before, after, language, fileName } = data;
  const lang = language || 'plaintext';
  const name = fileName || 'file';

  const combinedSize = before.length + after.length;
  const tooLarge = combinedSize > MAX_COMBINED_SIZE;

  const diffFile = useMemo(() => {
    if (tooLarge || before === after) return null;
    try {
      const file = generateDiffFile(name, before, name, after, lang, lang);
      file.initRaw();
      return file;
    } catch (e) {
      console.error('Failed to generate diff:', e);
      return null;
    }
  }, [name, before, after, lang, tooLarge]);

  const theme = document.documentElement.classList.contains('dark')
    ? 'dark'
    : 'light';

  const add = diffFile?.additionLength ?? 0;
  const del = diffFile?.deletionLength ?? 0;

  if (tooLarge) {
    return (
      <div className="my-2 rounded-md border border-border p-3 text-sm text-muted-foreground">
        Diff too large to display ({Math.round(combinedSize / 1024)}KB
        combined). View in editor.
      </div>
    );
  }

  if (before === after) {
    return (
      <div className="my-2 rounded-md border border-border p-3 text-sm text-muted-foreground">
        No differences
      </div>
    );
  }

  if (!diffFile) {
    return (
      <div className="my-2 rounded-md border border-border overflow-hidden">
        <div className="bg-muted px-3 py-1.5 text-xs font-mono text-muted-foreground">
          {name} (failed to render diff)
        </div>
        <pre className="p-3 text-xs font-mono overflow-x-auto whitespace-pre">
          {before}
        </pre>
      </div>
    );
  }

  return (
    <div className="my-2 rounded-md border border-border overflow-hidden">
      <div className="flex items-center gap-2 bg-muted px-3 py-1.5 text-xs font-mono text-muted-foreground">
        <span>{name}</span>
        <span className="text-green-500">+{add}</span>
        <span className="text-red-500">-{del}</span>
      </div>
      <div className="overflow-x-auto">
        <DiffView
          diffFile={diffFile}
          diffViewTheme={theme}
          diffViewHighlight
          diffViewMode={DiffModeEnum.Unified}
          diffViewFontSize={12}
        />
      </div>
    </div>
  );
}

const config: DecoratorNodeConfig<CodeDiffData> = {
  type: 'code-diff',
  serialization: {
    format: 'fenced',
    language: 'code-diff',
    serialize: (data) => JSON.stringify(data, null, 2),
    deserialize: (content) => JSON.parse(content),
    validate: (data) =>
      typeof data.before === 'string' && typeof data.after === 'string',
  },
  component: CodeDiffComponent,
};

const result = createDecoratorNode(config);

export const CodeDiffNode = result.Node;
export const $createCodeDiffNode = result.createNode;
export const $isCodeDiffNode = result.isNode;
export const [CODE_DIFF_EXPORT_TRANSFORMER, CODE_DIFF_TRANSFORMER] =
  result.transformers;
