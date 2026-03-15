import { useMemo } from 'react';
import { cn } from '@/lib/utils';
import {
  TrendingUp, TrendingDown, Minus, Users, Database, Clock, Gauge,
  Activity, Layers, AlertTriangle, CheckCircle, XCircle, AlertCircle,
} from 'lucide-react';

/**
 * Widget types supported (from METRICS.md documentation):
 * - number: Simple numeric value
 * - gauge: Circular percentage gauge (0-100)
 * - status: OK/Warning/Error indicator
 * - sparkline: Mini trend chart
 * - bars: Horizontal bar chart
 * - pie: Pie/donut chart
 * - table: Tabular data
 * - list: Simple list
 * - bar: Single progress bar
 */

interface MetricsWidgetsProps {
  metrics: Record<string, unknown>;
  className?: string;
}

// Get icon for metric based on key name
function getMetricIcon(key: string): React.ComponentType<{ className?: string }> {
  const k = key.toLowerCase();
  if (k.includes('user') || k.includes('connection') || k.includes('active') || k.includes('session')) return Users;
  if (k.includes('queue') || k.includes('depth') || k.includes('pending') || k.includes('lag')) return Layers;
  if (k.includes('latency') || k.includes('time') || k.includes('duration') || k.includes('ms')) return Clock;
  if (k.includes('memory') || k.includes('cache') || k.includes('storage') || k.includes('disk') || k.includes('heap')) return Database;
  if (k.includes('error') || k.includes('fail')) return AlertTriangle;
  if (k.includes('rate') || k.includes('throughput') || k.includes('request') || k.includes('message')) return Activity;
  return Gauge;
}

// Format number with appropriate precision
function formatNumber(value: number): string {
  if (Math.abs(value) >= 1000000) return `${(value / 1000000).toFixed(1)}M`;
  if (Math.abs(value) >= 1000) return `${(value / 1000).toFixed(1)}K`;
  if (Number.isInteger(value)) return value.toString();
  if (Math.abs(value) < 0.01) return value.toFixed(4);
  if (Math.abs(value) < 1) return value.toFixed(2);
  return value.toFixed(1);
}

// Infer unit from key name
function inferUnit(key: string): string {
  const k = key.toLowerCase();
  if (k.includes('_ms') || k.endsWith('ms') || k.includes('latency') || k.includes('duration')) return 'ms';
  if (k.includes('_mb') || k.endsWith('mb')) return 'MB';
  if (k.includes('_gb') || k.endsWith('gb')) return 'GB';
  if (k.includes('_pct') || k.includes('percent') || k.includes('ratio')) return '%';
  if (k.includes('per_sec') || k.includes('_sec') || k.includes('per_s') || k.includes('/s')) return '/s';
  if (k.includes('per_min') || k.includes('/min')) return '/min';
  if (k.includes('_mbps') || k.endsWith('mbps')) return 'Mbps';
  return '';
}

// Human-readable label from key
function formatLabel(key: string): string {
  return key
    .replace(/_widget$/, '')
    .replace(/_/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/\b\w/g, c => c.toUpperCase());
}

// ============================================================================
// Widget Components
// ============================================================================

function NumberWidget({ label, value, unit, icon: Icon }: {
  label: string;
  value: number;
  unit: string;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="flex items-center gap-2 mb-1">
        <Icon className="h-4 w-4 text-muted-foreground" />
        <span className="text-xs text-muted-foreground">{label}</span>
      </div>
      <div className="text-2xl font-bold">
        {formatNumber(value)}
        {unit && <span className="text-sm font-normal text-muted-foreground ml-1">{unit}</span>}
      </div>
    </div>
  );
}

function GaugeWidget({ label, value, icon: Icon }: {
  label: string;
  value: number;
  icon: React.ComponentType<{ className?: string }>;
}) {
  const percentage = Math.min(100, Math.max(0, value));
  const color = percentage >= 90 ? 'text-red-500' : percentage >= 70 ? 'text-orange-500' : 'text-emerald-500';
  const bgColor = percentage >= 90 ? 'stroke-red-500' : percentage >= 70 ? 'stroke-orange-500' : 'stroke-emerald-500';

  return (
    <div className="bg-muted/30 rounded-lg p-3 flex items-center gap-4">
      <div className="relative w-16 h-16">
        <svg className="w-16 h-16 -rotate-90" viewBox="0 0 64 64">
          <circle
            cx="32" cy="32" r="28"
            fill="none"
            stroke="currentColor"
            strokeWidth="6"
            className="text-muted/30"
          />
          <circle
            cx="32" cy="32" r="28"
            fill="none"
            strokeWidth="6"
            strokeDasharray={`${percentage * 1.76} 176`}
            strokeLinecap="round"
            className={bgColor}
          />
        </svg>
        <div className="absolute inset-0 flex items-center justify-center">
          <span className={cn('text-lg font-bold', color)}>{Math.round(value)}%</span>
        </div>
      </div>
      <div>
        <Icon className="h-4 w-4 text-muted-foreground mb-1" />
        <span className="text-sm">{label}</span>
      </div>
    </div>
  );
}

function StatusWidget({ label, value }: { label: string; value: string }) {
  const status = String(value).toLowerCase();
  const isOk = status === 'ok' || status === 'healthy' || status === 'running';
  const isWarning = status === 'warning' || status === 'degraded';
  const isCritical = status === 'error' || status === 'critical' || status === 'failed';

  const Icon = isOk ? CheckCircle : isWarning ? AlertCircle : isCritical ? XCircle : AlertCircle;
  const colorClass = isOk ? 'text-emerald-500 bg-emerald-500/10' :
                     isWarning ? 'text-orange-500 bg-orange-500/10' :
                     isCritical ? 'text-red-500 bg-red-500/10' :
                     'text-muted-foreground bg-muted/30';

  return (
    <div className={cn('rounded-lg p-3 flex items-center gap-3', colorClass)}>
      <Icon className="h-5 w-5" />
      <div>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="font-semibold capitalize">{String(value)}</div>
      </div>
    </div>
  );
}

function SparklineWidget({ label, values }: { label: string; values: number[] }) {
  if (!Array.isArray(values) || values.length === 0) return null;

  const max = Math.max(...values);
  const min = Math.min(...values);
  const range = max - min || 1;
  const height = 32;
  const width = 100;
  const points = values.map((v, i) => {
    const x = (i / (values.length - 1)) * width;
    const y = height - ((v - min) / range) * height;
    return `${x},${y}`;
  }).join(' ');

  const lastValue = values[values.length - 1];
  const prevValue = values.length > 1 ? values[values.length - 2] : lastValue;
  const trend = lastValue > prevValue ? 'up' : lastValue < prevValue ? 'down' : 'stable';

  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="flex items-center justify-between mb-2">
        <span className="text-xs text-muted-foreground">{label}</span>
        <div className="flex items-center gap-1">
          <span className="text-sm font-semibold">{formatNumber(lastValue)}</span>
          {trend === 'up' && <TrendingUp className="h-3 w-3 text-emerald-500" />}
          {trend === 'down' && <TrendingDown className="h-3 w-3 text-red-500" />}
          {trend === 'stable' && <Minus className="h-3 w-3 text-muted-foreground" />}
        </div>
      </div>
      <svg width={width} height={height} className="w-full">
        <polyline
          points={points}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          className="text-blue-500"
        />
      </svg>
    </div>
  );
}

function BarsWidget({ label, data }: { label: string; data: Record<string, number> }) {
  const entries = Object.entries(data);
  const max = Math.max(...entries.map(([_, v]) => v), 1);

  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="text-xs text-muted-foreground mb-2">{label}</div>
      <div className="space-y-2">
        {entries.map(([key, value]) => (
          <div key={key} className="space-y-1">
            <div className="flex justify-between text-xs">
              <span>{key}</span>
              <span className="font-medium">{formatNumber(value)}</span>
            </div>
            <div className="h-2 bg-muted/50 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 rounded-full transition-all"
                style={{ width: `${(value / max) * 100}%` }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function PieWidget({ label, data }: { label: string; data: Record<string, number> }) {
  const entries = Object.entries(data);
  const total = entries.reduce((sum, [_, v]) => sum + v, 0) || 1;
  const colors = ['#3B82F6', '#10B981', '#F59E0B', '#EF4444', '#8B5CF6', '#EC4899'];

  let currentAngle = 0;
  const slices = entries.map(([key, value], i) => {
    const angle = (value / total) * 360;
    const startAngle = currentAngle;
    currentAngle += angle;

    const startRad = (startAngle - 90) * Math.PI / 180;
    const endRad = (startAngle + angle - 90) * Math.PI / 180;
    const x1 = 50 + 40 * Math.cos(startRad);
    const y1 = 50 + 40 * Math.sin(startRad);
    const x2 = 50 + 40 * Math.cos(endRad);
    const y2 = 50 + 40 * Math.sin(endRad);
    const largeArc = angle > 180 ? 1 : 0;

    const path = `M 50 50 L ${x1} ${y1} A 40 40 0 ${largeArc} 1 ${x2} ${y2} Z`;

    return { key, value, path, color: colors[i % colors.length] };
  });

  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="text-xs text-muted-foreground mb-2">{label}</div>
      <div className="flex items-center gap-4">
        <svg viewBox="0 0 100 100" className="w-20 h-20">
          {slices.map((slice, i) => (
            <path key={i} d={slice.path} fill={slice.color} />
          ))}
        </svg>
        <div className="space-y-1 text-xs">
          {slices.map((slice, i) => (
            <div key={i} className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full" style={{ backgroundColor: slice.color }} />
              <span>{slice.key}: {formatNumber(slice.value)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function TableWidget({ label, data }: { label: string; data: Array<Record<string, unknown>> }) {
  if (!Array.isArray(data) || data.length === 0) return null;
  const columns = Object.keys(data[0]);

  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="text-xs text-muted-foreground mb-2">{label}</div>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="border-b">
              {columns.map(col => (
                <th key={col} className="text-left py-1 px-2 font-medium">{col}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.slice(0, 5).map((row, i) => (
              <tr key={i} className="border-b border-muted/30">
                {columns.map(col => (
                  <td key={col} className="py-1 px-2">{String(row[col] ?? '')}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ListWidget({ label, items }: { label: string; items: string[] }) {
  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="text-xs text-muted-foreground mb-2">{label}</div>
      <div className="flex flex-wrap gap-1">
        {items.slice(0, 10).map((item, i) => (
          <span key={i} className="text-xs px-2 py-0.5 bg-muted/50 rounded">
            {String(item)}
          </span>
        ))}
        {items.length > 10 && (
          <span className="text-xs text-muted-foreground">+{items.length - 10} more</span>
        )}
      </div>
    </div>
  );
}

function BarWidget({ label, value, max = 100 }: { label: string; value: number; max?: number }) {
  const percentage = Math.min(100, Math.max(0, (value / max) * 100));
  const color = percentage >= 90 ? 'bg-red-500' : percentage >= 70 ? 'bg-orange-500' : 'bg-emerald-500';

  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="flex justify-between text-xs mb-2">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium">{formatNumber(value)}</span>
      </div>
      <div className="h-2 bg-muted/50 rounded-full overflow-hidden">
        <div
          className={cn('h-full rounded-full transition-all', color)}
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}

function TextWidget({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-lg p-3">
      <div className="text-xs text-muted-foreground mb-1">{label}</div>
      <div className="text-sm whitespace-pre-wrap">{value}</div>
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

export function MetricsWidgets({ metrics, className }: MetricsWidgetsProps) {
  const widgets = useMemo(() => {
    const result: React.ReactNode[] = [];
    const processed = new Set<string>();

    // Process metrics in order, checking for _widget hints
    for (const [key, value] of Object.entries(metrics)) {
      if (key.endsWith('_widget') || processed.has(key)) continue;

      const widgetType = metrics[`${key}_widget`] as string | undefined;
      const label = formatLabel(key);
      const unit = inferUnit(key);
      const Icon = getMetricIcon(key);

      processed.add(key);
      processed.add(`${key}_widget`);

      // Render based on widget type hint or infer from data
      if (widgetType === 'gauge' || (key.toLowerCase().includes('percent') && typeof value === 'number')) {
        result.push(<GaugeWidget key={key} label={label} value={value as number} icon={Icon} />);
      } else if (widgetType === 'status' || (typeof value === 'string' && ['ok', 'warning', 'error', 'critical', 'healthy', 'failed'].includes(value.toLowerCase()))) {
        result.push(<StatusWidget key={key} label={label} value={value as string} />);
      } else if (widgetType === 'sparkline' || (Array.isArray(value) && value.every(v => typeof v === 'number'))) {
        result.push(<SparklineWidget key={key} label={label} values={value as number[]} />);
      } else if (widgetType === 'bars' || (typeof value === 'object' && value !== null && !Array.isArray(value) && Object.values(value as object).every(v => typeof v === 'number'))) {
        result.push(<BarsWidget key={key} label={label} data={value as Record<string, number>} />);
      } else if (widgetType === 'pie') {
        result.push(<PieWidget key={key} label={label} data={value as Record<string, number>} />);
      } else if (widgetType === 'table' || (Array.isArray(value) && value.length > 0 && typeof value[0] === 'object')) {
        result.push(<TableWidget key={key} label={label} data={value as Array<Record<string, unknown>>} />);
      } else if (widgetType === 'list' || (Array.isArray(value) && value.every(v => typeof v === 'string'))) {
        result.push(<ListWidget key={key} label={label} items={value as string[]} />);
      } else if (widgetType === 'bar') {
        result.push(<BarWidget key={key} label={label} value={value as number} />);
      } else if (widgetType === 'text') {
        result.push(<TextWidget key={key} label={label} value={String(value)} />);
      } else if (typeof value === 'number') {
        result.push(<NumberWidget key={key} label={label} value={value} unit={unit} icon={Icon} />);
      } else if (typeof value === 'string' && !isNaN(Number(value))) {
        result.push(<NumberWidget key={key} label={label} value={Number(value)} unit={unit} icon={Icon} />);
      }
      // Skip complex objects without widget hints
    }

    return result;
  }, [metrics]);

  if (widgets.length === 0) {
    return (
      <div className="text-center py-4 text-muted-foreground text-sm">
        No displayable metrics
      </div>
    );
  }

  return (
    <div className={cn('grid gap-3', className)}>
      {widgets}
    </div>
  );
}
