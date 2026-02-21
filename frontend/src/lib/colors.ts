export const STATE_COLORS = {
  RUNNING: { bg: '#E8F5E9', border: '#4CAF50', animation: 'none' },
  DEGRADED: { bg: '#FFF3E0', border: '#FF9800', animation: 'none' },
  FAILED: { bg: '#FFEBEE', border: '#F44336', animation: 'none' },
  STOPPED: { bg: '#F5F5F5', border: '#9E9E9E', animation: 'none' },
  STARTING: { bg: '#E3F2FD', border: '#2196F3', animation: 'pulse 1.5s ease-in-out infinite' },
  STOPPING: { bg: '#E3F2FD', border: '#2196F3', animation: 'pulse 1.5s ease-in-out infinite' },
  UNREACHABLE: { bg: 'rgba(33,33,33,0.1)', border: '#212121', animation: 'none' },
  UNKNOWN: { bg: '#FFFFFF', border: '#BDBDBD', animation: 'none', borderStyle: 'dashed' },
} as const;

export const COMPONENT_TYPE_ICONS = {
  database: { icon: 'Database', color: '#1565C0' },
  middleware: { icon: 'Layers', color: '#6A1B9A' },
  appserver: { icon: 'Server', color: '#2E7D32' },
  webfront: { icon: 'Globe', color: '#E65100' },
  service: { icon: 'Cog', color: '#37474F' },
  batch: { icon: 'Clock', color: '#4E342E' },
  custom: { icon: 'Box', color: '#455A64' },
} as const;

export const ERROR_BRANCH_COLORS = {
  bg: '#FFE0E6',
  border: '#FF6B8A',
} as const;

export type ComponentState = keyof typeof STATE_COLORS;
export type ComponentType = keyof typeof COMPONENT_TYPE_ICONS;
