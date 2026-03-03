import { NodeKey } from 'lexical';
import { useState, useEffect, useRef, useId } from 'react';
import { Loader2 } from 'lucide-react';
import {
  createDecoratorNode,
  type DecoratorNodeConfig,
} from '../lib/create-decorator-node';

interface MermaidData {
  source: string;
}

function MermaidComponent({
  data,
}: {
  data: MermaidData;
  nodeKey: NodeKey;
  onDoubleClickEdit: (event: React.MouseEvent) => void;
}) {
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const containerRef = useRef<HTMLDivElement>(null);
  const uniqueId = useId();
  const renderId = `mermaid-${uniqueId.replace(/:/g, '-')}`;

  useEffect(() => {
    let cancelled = false;

    async function render() {
      setLoading(true);
      setError(null);
      setSvg(null);

      try {
        const mermaid = (await import('mermaid')).default;
        const theme = document.documentElement.classList.contains('dark')
          ? 'dark'
          : 'default';
        mermaid.initialize({
          startOnLoad: false,
          theme,
          securityLevel: 'strict',
        });
        const { svg: rendered } = await mermaid.render(
          renderId,
          data.source.trim()
        );
        if (!cancelled) {
          setSvg(rendered);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : 'Failed to render diagram');
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    render();
    return () => {
      cancelled = true;
    };
  }, [data.source, renderId]);

  if (loading) {
    return (
      <div className="my-2 flex items-center justify-center rounded-md border border-border p-6">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="my-2 rounded-md border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-950/30 overflow-hidden">
        <div className="px-3 py-1.5 text-xs font-medium text-red-600 dark:text-red-400 border-b border-red-200 dark:border-red-800">
          Mermaid error
        </div>
        <pre className="p-3 text-xs font-mono overflow-x-auto whitespace-pre text-muted-foreground">
          {data.source}
        </pre>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="my-2 rounded-md border border-border p-3 overflow-x-auto [&>svg]:max-w-full"
      dangerouslySetInnerHTML={{ __html: svg ?? '' }}
    />
  );
}

const config: DecoratorNodeConfig<MermaidData> = {
  type: 'mermaid',
  serialization: {
    format: 'fenced',
    language: 'mermaid',
    serialize: (data) => data.source,
    deserialize: (content) => ({ source: content }),
    validate: (data) => !!data.source.trim(),
  },
  component: MermaidComponent,
};

const result = createDecoratorNode(config);

export const MermaidNode = result.Node;
export const $createMermaidNode = result.createNode;
export const $isMermaidNode = result.isNode;
export const [MERMAID_EXPORT_TRANSFORMER, MERMAID_TRANSFORMER] =
  result.transformers;
