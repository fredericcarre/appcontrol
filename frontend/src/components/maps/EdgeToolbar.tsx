import { useCallback, useEffect, useRef, useState } from 'react';
import { useReactFlow } from '@xyflow/react';
import { Trash2, ArrowRight } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface EdgeToolbarProps {
  edgeId: string;
  sourceId: string;
  targetId: string;
  sourceName: string;
  targetName: string;
  onDelete: (edgeId: string) => void;
}

/**
 * Floating toolbar that appears above a selected edge in edit mode.
 * Shows source->target label and a delete button.
 */
export function EdgeToolbar({
  edgeId,
  sourceId,
  targetId,
  sourceName,
  targetName,
  onDelete,
}: EdgeToolbarProps) {
  const { getNodes } = useReactFlow();
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const nodes = getNodes();
    const sourceNode = nodes.find((n) => n.id === sourceId);
    const targetNode = nodes.find((n) => n.id === targetId);
    if (sourceNode && targetNode) {
      // Position at midpoint between the two nodes
      const midX = (sourceNode.position.x + targetNode.position.x) / 2 + 90;
      const midY = (sourceNode.position.y + targetNode.position.y) / 2;
      setPosition({ x: midX, y: midY });
    }
  }, [getNodes, sourceId, targetId]);

  const handleDelete = useCallback(() => {
    onDelete(edgeId);
  }, [edgeId, onDelete]);

  if (!position) return null;

  return (
    <div
      ref={toolbarRef}
      className="absolute z-50 pointer-events-auto"
      style={{
        transform: `translate(${position.x}px, ${position.y}px) translate(-50%, -100%)`,
      }}
    >
      <div className="flex items-center gap-1 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700 px-2 py-1.5 text-xs">
        <span className="text-muted-foreground truncate max-w-[80px]" title={sourceName}>
          {sourceName}
        </span>
        <ArrowRight className="h-3 w-3 text-muted-foreground shrink-0" />
        <span className="text-muted-foreground truncate max-w-[80px]" title={targetName}>
          {targetName}
        </span>
        <div className="w-px h-4 bg-gray-200 mx-1" />
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0 text-red-500 hover:text-red-700 hover:bg-red-50"
          onClick={handleDelete}
          title="Delete dependency (Delete)"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}
