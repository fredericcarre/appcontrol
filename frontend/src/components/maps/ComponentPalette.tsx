import { useMemo, useState } from 'react';
import { GripVertical, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import { resolveIcon } from '@/lib/icons';
import { mergeCatalogIntoIcons } from '@/lib/colors';
import { useCatalog, CatalogEntry } from '@/api/catalog';
import { Input } from '@/components/ui/input';

export interface ComponentTypeDefinition {
  type: string;
  label: string;
  icon: React.ElementType;
  iconName: string;
  color: string;
  description: string;
  category?: string;
  defaultCheckCmd?: string;
  defaultStartCmd?: string;
  defaultStopCmd?: string;
  defaultEnvVars?: Record<string, string>;
  isBuiltin?: boolean;
}

/** Static fallback types — used when catalog API is not yet loaded. */
const FALLBACK_TYPES: ComponentTypeDefinition[] = [
  { type: 'database',    label: 'Database',    icon: resolveIcon('database'), iconName: 'database', color: '#1565C0', description: 'SQL, NoSQL, or data stores' },
  { type: 'middleware',  label: 'Middleware',   icon: resolveIcon('layers'),   iconName: 'layers',   color: '#6A1B9A', description: 'Message queues, cache, ESB' },
  { type: 'appserver',   label: 'App Server',  icon: resolveIcon('server'),   iconName: 'server',   color: '#2E7D32', description: 'Application servers, backends' },
  { type: 'webfront',    label: 'Web Front',   icon: resolveIcon('globe'),    iconName: 'globe',    color: '#E65100', description: 'Web servers, load balancers' },
  { type: 'service',     label: 'Service',     icon: resolveIcon('cog'),      iconName: 'cog',      color: '#37474F', description: 'Microservices, APIs' },
  { type: 'batch',       label: 'Batch',       icon: resolveIcon('clock'),    iconName: 'clock',    color: '#4E342E', description: 'Scheduled jobs, ETL' },
  { type: 'custom',      label: 'Custom',      icon: resolveIcon('box'),      iconName: 'box',      color: '#455A64', description: 'Other component types' },
  { type: 'application', label: 'Application', icon: resolveIcon('folder'),   iconName: 'folder',   color: '#3B82F6', description: 'Reference to another app (synthetic)' },
];

/** Convert a catalog API entry into a ComponentTypeDefinition. */
function catalogToDefinition(entry: CatalogEntry): ComponentTypeDefinition {
  return {
    type: entry.type_key,
    label: entry.label,
    icon: resolveIcon(entry.icon),
    iconName: entry.icon,
    color: entry.color,
    description: entry.description || '',
    category: entry.category || undefined,
    defaultCheckCmd: entry.default_check_cmd || undefined,
    defaultStartCmd: entry.default_start_cmd || undefined,
    defaultStopCmd: entry.default_stop_cmd || undefined,
    defaultEnvVars: entry.default_env_vars || undefined,
    isBuiltin: entry.is_builtin,
  };
}

/** Hook to get component types from catalog, with fallback. */
export function useComponentTypes(): {
  types: ComponentTypeDefinition[];
  isLoading: boolean;
  byCategory: Record<string, ComponentTypeDefinition[]>;
} {
  const { data: catalog, isLoading } = useCatalog();

  const types = useMemo(() => {
    if (!catalog || catalog.length === 0) return FALLBACK_TYPES;
    // Merge catalog entries into the global COMPONENT_TYPE_ICONS map
    // so ComponentNode can also resolve custom types
    mergeCatalogIntoIcons(catalog.map((e) => ({ type_key: e.type_key, icon: e.icon, color: e.color })));
    return catalog.map(catalogToDefinition);
  }, [catalog]);

  const byCategory = useMemo(() => {
    const groups: Record<string, ComponentTypeDefinition[]> = {};
    for (const t of types) {
      const cat = t.category || 'other';
      if (!groups[cat]) groups[cat] = [];
      groups[cat].push(t);
    }
    return groups;
  }, [types]);

  return { types, isLoading, byCategory };
}

// Keep backward-compatible export
export const COMPONENT_TYPES = FALLBACK_TYPES;

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
