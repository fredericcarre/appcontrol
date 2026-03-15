import { useMemo } from 'react';
import { cn } from '@/lib/utils';
import { TrendingUp, TrendingDown, Minus, Users, Database, Clock, Gauge, Activity, Layers, AlertTriangle } from 'lucide-react';

/**
 * Widget types for metric display:
 * - number: Simple numeric value with optional unit
 * - gauge: Circular gauge for percentage values (0-100)
 * - bar: Horizontal progress bar
 * - trend: Number with trend indicator (up/down/stable)
 * - list: Key-value pairs
 * - status: OK/Warning/Critical indicator
 */
export type MetricWidgetType = 'number' | 'gauge' | 'bar' | 'trend' | 'list' | 'status' | 'auto';

export interface MetricWidget {
  key: string;           // The metric key to display
  label?: string;        // Display label (defaults to key)
  type?: MetricWidgetType; // Widget type (defaults to 'auto')
  unit?: string;         // Unit suffix (e.g., 'ms', '%', 'MB')
  min?: number;          // Min value for gauge/bar
  max?: number;          // Max value for gauge/bar
  thresholds?: {         // Color thresholds
    warning?: number;
    critical?: number;
  };
  icon?: string;         // Icon name
}

interface MetricsDisplayProps {
  metrics: Record<string, unknown> | null;
  widgets?: MetricWidget[];
  compact?: boolean;
  className?: string;
}

// Infer widget type from metric key and value
function inferWidgetType(key: string, value: unknown): MetricWidgetType {
  const keyLower = key.toLowerCase();

  // Percentage indicators
  if (keyLower.includes('ratio') || keyLower.includes('rate') || keyLower.includes('pct') || keyLower.includes('percent')) {
    return 'gauge';
  }

  // Latency/timing
  if (keyLower.includes('latency') || keyLower.includes('duration') || keyLower.includes('_ms') || keyLower.includes('time')) {
    return 'trend';
  }

  // Counts
  if (keyLower.includes('count') || keyLower.includes('total') || keyLower.includes('active') || keyLower.includes('connections')) {
    return 'number';
  }

  // Arrays become lists
  if (Array.isArray(value)) {
    return 'list';
  }

  // Objects become lists
  if (typeof value === 'object' && value !== null) {
    return 'list';
  }

  return 'number';
}

// Infer unit from metric key
function inferUnit(key: string): string | undefined {
  const keyLower = key.toLowerCase();
  if (keyLower.includes('_ms') || keyLower.includes('latency') || keyLower.includes('duration')) return 'ms';
  if (keyLower.includes('_mb') || keyLower.includes('memory_mb')) return 'MB';
  if (keyLower.includes('_gb')) return 'GB';
  if (keyLower.includes('_pct') || keyLower.includes('percent') || keyLower.includes('ratio')) return '%';
  if (keyLower.includes('per_sec') || keyLower.includes('_sec') || keyLower.includes('per_s')) return '/s';
  if (keyLower.includes('per_min')) return '/min';
  return undefined;
}

// Get icon for metric
function getMetricIcon(key: string): React.ComponentType<{ className?: string }> {
  const keyLower = key.toLowerCase();
  if (keyLower.includes('user') || keyLower.includes('connection') || keyLower.includes('active')) return Users;
  if (keyLower.includes('queue') || keyLower.includes('depth') || keyLower.includes('pending')) return Layers;
  if (keyLower.includes('latency') || keyLower.includes('time') || keyLower.includes('duration')) return Clock;
  if (keyLower.includes('memory') || keyLower.includes('cache') || keyLower.includes('storage')) return Database;
  if (keyLower.includes('error') || keyLower.includes('fail')) return AlertTriangle;
  if (keyLower.includes('rate') || keyLower.includes('throughput') || keyLower.includes('request')) return Activity;
  return Gauge;
}

// Format number with appropriate precision
function formatNumber(value: number): string {
  if (Math.abs(value) >= 1000000) return `${(value / 1000000).toFixed(1)}M`;
  if (Math.abs(value) >= 1000) return `${(value / 1000).toFixed(1)}K`;
  if (Number.isInteger(value)) return value.toString();
  if (Math.abs(value) < 1) return value.toFixed(3);
  return value.toFixed(1);
}

// Get color based on value and thresholds
function getValueColor(value: number, thresholds?: { warning?: number; critical?: number }, inverted?: boolean): string {
  if (!thresholds) return 'text-foreground';
  const { warning, critical } = thresholds;

  if (critical !== undefined && (inverted ? value <= critical : value >= critical)) {
    return 'text-red-600';
  }
  if (warning !== undefined && (inverted ? value <= warning : value >= warning)) {
    return 'text-orange-500';
  }
  return 'text-green-600';
}

// Render a single metric widget
function MetricWidget({
  metricKey,
  value,
  widget,
  compact
}: {
  metricKey: string;
  value: unknown;
  widget?: MetricWidget;
  compact?: boolean;
}) {
  const type = widget?.type === 'auto' || !widget?.type
    ? inferWidgetType(metricKey, value)
    : widget.type;
  const unit = widget?.unit || inferUnit(metricKey);
  const label = widget?.label || metricKey.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
  const Icon = getMetricIcon(metricKey);

  const numValue = typeof value === 'number' ? value : parseFloat(String(value));
  const isValidNumber = !isNaN(numValue);

  // For gauge/bar, calculate percentage
  const min = widget?.min ?? 0;
  const max = widget?.max ?? (unit === '%' ? 100 : 100);
  const percentage = isValidNumber ? Math.min(100, Math.max(0, ((numValue - min) / (max - min)) * 100)) : 0;

  const valueColor = isValidNumber ? getValueColor(numValue, widget?.thresholds) : 'text-muted-foreground';

  if (compact) {
    // Compact mode: single line with icon, value, and unit
    // Handle complex types (objects, arrays) specially
    let displayValue: string;
    if (isValidNumber) {
      displayValue = formatNumber(numValue);
    } else if (Array.isArray(value)) {
      displayValue = `[${value.length}]`;
    } else if (typeof value === 'object' && value !== null) {
      displayValue = `{${Object.keys(value).length}}`;
    } else {
      displayValue = String(value).slice(0, 12);
    }

    return (
      <div className="flex items-center gap-1 text-[10px]">
        <Icon className="h-2.5 w-2.5 text-muted-foreground" />
        <span className={cn('font-medium', valueColor)}>
          {displayValue}
        </span>
        {unit && <span className="text-muted-foreground">{unit}</span>}
      </div>
    );
  }

  switch (type) {
    case 'gauge':
      return (
        <div className="flex items-center gap-2">
          <div className="relative w-8 h-8">
            <svg className="w-8 h-8 -rotate-90" viewBox="0 0 32 32">
              <circle
                cx="16" cy="16" r="12"
                fill="none"
                stroke="currentColor"
                strokeWidth="4"
                className="text-muted/20"
              />
              <circle
                cx="16" cy="16" r="12"
                fill="none"
                stroke="currentColor"
                strokeWidth="4"
                strokeDasharray={`${percentage * 0.75} 100`}
                className={valueColor}
              />
            </svg>
            <span className={cn('absolute inset-0 flex items-center justify-center text-[8px] font-bold', valueColor)}>
              {Math.round(numValue)}
            </span>
          </div>
          <div className="flex flex-col">
            <span className="text-[9px] text-muted-foreground">{label}</span>
          </div>
        </div>
      );

    case 'bar':
      return (
        <div className="space-y-0.5">
          <div className="flex justify-between text-[9px]">
            <span className="text-muted-foreground">{label}</span>
            <span className={valueColor}>{formatNumber(numValue)}{unit}</span>
          </div>
          <div className="h-1.5 bg-muted/20 rounded-full overflow-hidden">
            <div
              className={cn('h-full rounded-full transition-all',
                valueColor.replace('text-', 'bg-')
              )}
              style={{ width: `${percentage}%` }}
            />
          </div>
        </div>
      );

    case 'trend':
      // Simulate trend (in real app, compare with previous value)
      const trend = numValue > 50 ? 'up' : numValue < 30 ? 'down' : 'stable';
      return (
        <div className="flex items-center gap-1.5">
          <Icon className="h-3 w-3 text-muted-foreground" />
          <span className={cn('text-sm font-medium', valueColor)}>
            {formatNumber(numValue)}
          </span>
          {unit && <span className="text-[10px] text-muted-foreground">{unit}</span>}
          {trend === 'up' && <TrendingUp className="h-3 w-3 text-red-500" />}
          {trend === 'down' && <TrendingDown className="h-3 w-3 text-green-500" />}
          {trend === 'stable' && <Minus className="h-3 w-3 text-muted-foreground" />}
        </div>
      );

    case 'list':
      const items = Array.isArray(value) ? value : Object.entries(value as object);
      return (
        <div className="space-y-0.5">
          <span className="text-[9px] text-muted-foreground">{label}</span>
          <div className="flex flex-wrap gap-1">
            {(items as unknown[]).slice(0, 3).map((item, i) => (
              <span key={i} className="text-[9px] px-1 py-0.5 bg-muted/30 rounded">
                {Array.isArray(value) ? String(item) : `${(item as [string, unknown])[0]}: ${(item as [string, unknown])[1]}`}
              </span>
            ))}
            {items.length > 3 && (
              <span className="text-[9px] text-muted-foreground">+{items.length - 3}</span>
            )}
          </div>
        </div>
      );

    case 'status':
      const status = numValue === 0 ? 'ok' : numValue < (widget?.thresholds?.warning ?? 1) ? 'warning' : 'critical';
      return (
        <div className="flex items-center gap-1.5">
          <div className={cn(
            'w-2 h-2 rounded-full',
            status === 'ok' && 'bg-green-500',
            status === 'warning' && 'bg-orange-500',
            status === 'critical' && 'bg-red-500',
          )} />
          <span className="text-[10px]">{label}</span>
        </div>
      );

    default: // 'number'
      return (
        <div className="flex items-center gap-1.5">
          <Icon className="h-3 w-3 text-muted-foreground" />
          <div className="flex flex-col">
            <span className={cn('text-sm font-medium leading-none', valueColor)}>
              {isValidNumber ? formatNumber(numValue) : String(value)}
              {unit && <span className="text-[10px] text-muted-foreground ml-0.5">{unit}</span>}
            </span>
            <span className="text-[9px] text-muted-foreground">{label}</span>
          </div>
        </div>
      );
  }
}

export function MetricsDisplay({ metrics, widgets, compact = false, className }: MetricsDisplayProps) {
  // Build the list of metrics to display
  const displayMetrics = useMemo(() => {
    if (!metrics) return [];

    // If widgets are defined, use them in order
    if (widgets && widgets.length > 0) {
      return widgets
        .filter(w => w.key in metrics)
        .map(w => ({ key: w.key, value: metrics[w.key], widget: w }));
    }

    // Otherwise, auto-discover all metrics (excluding _widget hint keys)
    return Object.entries(metrics)
      .filter(([k, v]) => v !== null && v !== undefined && !k.endsWith('_widget'))
      .slice(0, compact ? 3 : 6) // Limit display
      .map(([key, value]) => ({ key, value, widget: undefined }));
  }, [metrics, widgets, compact]);

  if (displayMetrics.length === 0) return null;

  return (
    <div className={cn(
      'grid gap-2',
      compact ? 'grid-cols-3' : 'grid-cols-2',
      className
    )}>
      {displayMetrics.map(({ key, value, widget }) => (
        <MetricWidget
          key={key}
          metricKey={key}
          value={value}
          widget={widget}
          compact={compact}
        />
      ))}
    </div>
  );
}
