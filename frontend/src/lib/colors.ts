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

export const COMPONENT_TYPE_ICONS: Record<string, { icon: string; color: string }> = {
  // Standard types
  database: { icon: 'Database', color: '#1565C0' },
  middleware: { icon: 'Layers', color: '#6A1B9A' },
  appserver: { icon: 'Server', color: '#2E7D32' },
  webfront: { icon: 'Globe', color: '#E65100' },
  service: { icon: 'Cog', color: '#37474F' },
  batch: { icon: 'Clock', color: '#4E342E' },
  custom: { icon: 'Box', color: '#455A64' },
  // Common aliases (flexible types)
  db: { icon: 'Database', color: '#1565C0' },
  application: { icon: 'Server', color: '#2E7D32' },
  app: { icon: 'Server', color: '#2E7D32' },
  server: { icon: 'Server', color: '#2E7D32' },
  webserver: { icon: 'Globe', color: '#E65100' },
  web: { icon: 'Globe', color: '#E65100' },
  frontend: { icon: 'Globe', color: '#E65100' },
  api: { icon: 'Cog', color: '#37474F' },
  svc: { icon: 'Cog', color: '#37474F' },
  job: { icon: 'Clock', color: '#4E342E' },
  scheduler: { icon: 'Clock', color: '#4E342E' },
  loadbalancer: { icon: 'Network', color: '#0277BD' },
  lb: { icon: 'Network', color: '#0277BD' },
  proxy: { icon: 'Network', color: '#0277BD' },
  gateway: { icon: 'Network', color: '#0277BD' },
  cache: { icon: 'Zap', color: '#F57C00' },
  redis: { icon: 'Zap', color: '#DC382D' },
  memcached: { icon: 'Zap', color: '#F57C00' },
  mq: { icon: 'Layers', color: '#6A1B9A' },
  queue: { icon: 'Layers', color: '#6A1B9A' },
  messaging: { icon: 'Layers', color: '#6A1B9A' },
};

export const ERROR_BRANCH_COLORS = {
  bg: '#FFE0E6',
  border: '#FF6B8A',
} as const;

export type ComponentState = keyof typeof STATE_COLORS;
export type ComponentType = string; // Flexible: any type is allowed

export const TECHNOLOGY_COLORS: Record<string, string> = {
  postgresql: '#336791',
  mysql: '#4479A1',
  oracle: '#F80000',
  sqlserver: '#CC2927',
  redis: '#DC382D',
  mongodb: '#47A248',
  kafka: '#231F20',
  rabbitmq: '#FF6600',
  activemq: '#D5382F',
  elasticsearch: '#FEC514',
  http: '#4CAF50',
  https: '#2E7D32',
  ldap: '#7B1FA2',
  smtp: '#1565C0',
  ftp: '#795548',
  ssh: '#263238',
  default: '#94a3b8',
};
