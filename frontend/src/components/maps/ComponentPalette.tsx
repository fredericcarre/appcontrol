import { Database, Layers, Server, Globe, Cog, Clock, Box, GripVertical } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ComponentTypeDefinition {
  type: string;
  label: string;
  icon: React.ElementType;
  iconName: string;
  color: string;
  description: string;
}

export const COMPONENT_TYPES: ComponentTypeDefinition[] = [
  {
    type: 'database',
    label: 'Database',
    icon: Database,
    iconName: 'database',
    color: '#1565C0',
    description: 'SQL, NoSQL, or data stores',
  },
  {
    type: 'middleware',
    label: 'Middleware',
    icon: Layers,
    iconName: 'layers',
    color: '#6A1B9A',
    description: 'Message queues, cache, ESB',
  },
  {
    type: 'appserver',
    label: 'App Server',
    icon: Server,
    iconName: 'server',
    color: '#2E7D32',
    description: 'Application servers, backends',
  },
  {
    type: 'webfront',
    label: 'Web Front',
    icon: Globe,
    iconName: 'globe',
    color: '#E65100',
    description: 'Web servers, load balancers',
  },
  {
    type: 'service',
    label: 'Service',
    icon: Cog,
    iconName: 'cog',
    color: '#37474F',
    description: 'Microservices, APIs',
  },
  {
    type: 'batch',
    label: 'Batch',
    icon: Clock,
    iconName: 'clock',
    color: '#4E342E',
    description: 'Scheduled jobs, ETL',
  },
  {
    type: 'custom',
    label: 'Custom',
    icon: Box,
    iconName: 'box',
    color: '#455A64',
    description: 'Other component types',
  },
];

interface ComponentPaletteProps {
  className?: string;
  onDragStart?: (event: React.DragEvent, type: string) => void;
}

export function ComponentPalette({ className, onDragStart }: ComponentPaletteProps) {
  const handleDragStart = (event: React.DragEvent, compType: ComponentTypeDefinition) => {
    event.dataTransfer.setData('application/reactflow', compType.type);
    event.dataTransfer.setData('text/plain', compType.type);
    event.dataTransfer.effectAllowed = 'move';
    onDragStart?.(event, compType.type);
  };

  return (
    <div className={cn('bg-card border rounded-lg p-3 shadow-lg', className)}>
      <h3 className="text-sm font-semibold mb-3 text-muted-foreground uppercase tracking-wider">
        Components
      </h3>
      <div className="space-y-2">
        {COMPONENT_TYPES.map((compType) => {
          const Icon = compType.icon;
          return (
            <div
              key={compType.type}
              draggable
              onDragStart={(e) => handleDragStart(e, compType)}
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
        })}
      </div>
      <p className="text-xs text-muted-foreground mt-3">
        Drag components onto the canvas to add them
      </p>
    </div>
  );
}
