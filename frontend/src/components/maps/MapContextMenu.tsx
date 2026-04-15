import { useCallback, useEffect, useRef } from 'react';
import { Trash2, Link, Pencil, Unlink } from 'lucide-react';

interface ContextMenuPosition {
  x: number;
  y: number;
}

interface EdgeContextMenuProps {
  type: 'edge';
  position: ContextMenuPosition;
  edgeId: string;
  sourceName: string;
  targetName: string;
  onDelete: (edgeId: string) => void;
  onClose: () => void;
}

interface NodeContextMenuProps {
  type: 'node';
  position: ContextMenuPosition;
  nodeId: string;
  nodeName: string;
  onDelete: (nodeId: string) => void;
  onEdit: (nodeId: string) => void;
  onStartConnect: (nodeId: string) => void;
  onClose: () => void;
}

export type MapContextMenuProps = EdgeContextMenuProps | NodeContextMenuProps;

export function MapContextMenu(props: MapContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  const handleClose = useCallback(() => {
    props.onClose();
  }, [props]);

  // Close on click outside or Escape
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        handleClose();
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') handleClose();
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [handleClose]);

  if (props.type === 'edge') {
    return (
      <div
        ref={menuRef}
        className="fixed z-[200] min-w-[180px] bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 text-sm animate-in fade-in-0 zoom-in-95"
        style={{ left: props.position.x, top: props.position.y }}
      >
        <div className="px-3 py-1.5 text-xs text-muted-foreground border-b mb-1">
          <span className="font-medium">{props.sourceName}</span>
          {' → '}
          <span className="font-medium">{props.targetName}</span>
        </div>
        <button
          className="flex items-center gap-2 w-full px-3 py-1.5 hover:bg-red-50 dark:hover:bg-red-900/20 text-red-600 transition-colors"
          onClick={() => {
            props.onDelete(props.edgeId);
            handleClose();
          }}
        >
          <Unlink className="h-4 w-4" />
          Delete dependency
          <span className="ml-auto text-xs text-muted-foreground">Del</span>
        </button>
      </div>
    );
  }

  // Node context menu
  return (
    <div
      ref={menuRef}
      className="fixed z-[200] min-w-[180px] bg-white dark:bg-gray-800 rounded-lg shadow-xl border border-gray-200 dark:border-gray-700 py-1 text-sm animate-in fade-in-0 zoom-in-95"
      style={{ left: props.position.x, top: props.position.y }}
    >
      <div className="px-3 py-1.5 text-xs text-muted-foreground border-b mb-1 font-medium">
        {props.nodeName}
      </div>
      <button
        className="flex items-center gap-2 w-full px-3 py-1.5 hover:bg-accent transition-colors"
        onClick={() => {
          props.onEdit(props.nodeId);
          handleClose();
        }}
      >
        <Pencil className="h-4 w-4" />
        Edit properties
      </button>
      <button
        className="flex items-center gap-2 w-full px-3 py-1.5 hover:bg-accent transition-colors"
        onClick={() => {
          props.onStartConnect(props.nodeId);
          handleClose();
        }}
      >
        <Link className="h-4 w-4" />
        Connect to...
      </button>
      <div className="h-px bg-gray-200 dark:bg-gray-700 my-1" />
      <button
        className="flex items-center gap-2 w-full px-3 py-1.5 hover:bg-red-50 dark:hover:bg-red-900/20 text-red-600 transition-colors"
        onClick={() => {
          props.onDelete(props.nodeId);
          handleClose();
        }}
      >
        <Trash2 className="h-4 w-4" />
        Delete component
        <span className="ml-auto text-xs text-muted-foreground">Del</span>
      </button>
    </div>
  );
}
