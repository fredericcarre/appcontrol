import { useMemo } from 'react';
import { resolveIcon } from '@/lib/icons';
import { mergeCatalogIntoIcons } from '@/lib/colors';
import { useCatalog, CatalogEntry } from '@/api/catalog';

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
export const FALLBACK_TYPES: ComponentTypeDefinition[] = [
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
