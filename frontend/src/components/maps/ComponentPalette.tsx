import { useMemo, useState } from 'react';
import { GripVertical, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Input } from '@/components/ui/input';
import { useComponentTypes, ComponentTypeDefinition } from '@/hooks/use-component-types';

interface ComponentPaletteProps {
  className?: string;
  onDragStart?: (event: React.DragEvent, type: string) => void;
}

export function ComponentPalette({ className, onDragStart }: ComponentPaletteProps) {
  const { types, byCategory } = useComponentTypes();
  const [search, setSearch] = useState('');

  const filteredTypes = useMemo(() => {
    if (!search.trim()) return types;
    const q = search.toLowerCase();
    return types.filter(
      (t) =>
        t.label.toLowerCase().includes(q) ||
        t.type.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q) ||
        (t.category && t.category.toLowerCase().includes(q)),
    );
  }, [types, search]);

  const handleDragStart = (event: React.DragEvent, compType: ComponentTypeDefinition) => {
    event.dataTransfer.setData('application/reactflow', compType.type);
    event.dataTransfer.setData('text/plain', compType.type);
    event.dataTransfer.effectAllowed = 'move';
    onDragStart?.(event, compType.type);
  };

  // Group filtered types by category for display
  const grouped = useMemo(() => {
    const g: Record<string, ComponentTypeDefinition[]> = {};
    for (const t of filteredTypes) {
      const cat = t.category || 'other';
      if (!g[cat]) g[cat] = [];
      g[cat].push(t);
    }
    return g;
  }, [filteredTypes]);

  const hasCategories = Object.keys(byCategory).length > 1;

  return (
    <div className={cn('bg-card border rounded-lg p-3 shadow-lg', className)}>
      <h3 className="text-sm font-semibold mb-2 text-muted-foreground uppercase tracking-wider">
        Components
      </h3>

      {types.length > 8 && (
        <div className="relative mb-3">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter types..."
            className="h-7 pl-7 text-xs"
          />
        </div>
      )}

      <div className="space-y-1 max-h-[60vh] overflow-y-auto">
        {hasCategories && !search.trim()
          ? Object.entries(grouped).map(([cat, items]) => (
              <div key={cat}>
                <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider px-1 pt-2 pb-1">
                  {cat}
                </div>
                {items.map((compType) => (
                  <PaletteItem
                    key={compType.type}
                    compType={compType}
                    onDragStart={handleDragStart}
                  />
                ))}
              </div>
            ))
          : filteredTypes.map((compType) => (
              <PaletteItem
                key={compType.type}
                compType={compType}
                onDragStart={handleDragStart}
              />
            ))}
        {filteredTypes.length === 0 && (
          <p className="text-xs text-muted-foreground py-2 text-center">
            No matching types
          </p>
        )}
      </div>
      <p className="text-xs text-muted-foreground mt-3">
        Drag components onto the canvas to add them
      </p>
    </div>
  );
}

function PaletteItem({
  compType,
  onDragStart,
}: {
  compType: ComponentTypeDefinition;
  onDragStart: (event: React.DragEvent, compType: ComponentTypeDefinition) => void;
}) {
  const Icon = compType.icon;
  return (
    <div
      draggable
      onDragStart={(e) => onDragStart(e, compType)}
      className="flex items-center gap-2 p-2 rounded-md border border-transparent hover:border-border hover:bg-accent cursor-grab active:cursor-grabbing transition-colors group"
      title={compType.description}
    >
      <GripVertical className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
      <div
        className="w-8 h-8 rounded flex items-center justify-center"
        style={{ backgroundColor: `${compType.color}20` }}
      >
        <Icon className="h-4 w-4" style={{ color: compType.color }} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate">{compType.label}</div>
      </div>
    </div>
  );
}
