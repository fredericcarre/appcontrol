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
  application: { icon: 'Folder', color: '#3B82F6' }, // Composite app (reference to another app)
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

/**
 * Merge catalog entries into the runtime icon map.
 * Call this after loading catalog from API to ensure ComponentNode
 * can resolve custom types.
 */
export function mergeCatalogIntoIcons(
  entries: Array<{ type_key: string; icon: string; color: string }>,
): void {
  for (const e of entries) {
    if (!COMPONENT_TYPE_ICONS[e.type_key]) {
      COMPONENT_TYPE_ICONS[e.type_key] = { icon: e.icon, color: e.color };
    }
  }
}

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

// Technology icons mapping (icon name from tech_patterns.rs → lucide icon + color)
// These are used by discovery to display recognized technologies
export const TECHNOLOGY_ICONS: Record<string, { icon: string; color: string; label: string }> = {
  // Databases
  oracle: { icon: 'Database', color: '#F80000', label: 'Oracle' },
  sqlserver: { icon: 'Database', color: '#CC2927', label: 'SQL Server' },
  db2: { icon: 'Database', color: '#054ADA', label: 'IBM DB2' },
  sybase: { icon: 'Database', color: '#003545', label: 'SAP ASE' },
  mysql: { icon: 'Database', color: '#4479A1', label: 'MySQL' },
  postgresql: { icon: 'Database', color: '#336791', label: 'PostgreSQL' },
  mongodb: { icon: 'Database', color: '#47A248', label: 'MongoDB' },
  redis: { icon: 'Zap', color: '#DC382D', label: 'Redis' },
  elastic: { icon: 'Search', color: '#FEC514', label: 'ElasticSearch' },
  elasticsearch: { icon: 'Search', color: '#FEC514', label: 'ElasticSearch' },

  // Middleware - IBM
  ibmmq: { icon: 'Layers', color: '#054ADA', label: 'IBM MQ' },
  websphere: { icon: 'Server', color: '#054ADA', label: 'WebSphere' },
  liberty: { icon: 'Server', color: '#054ADA', label: 'Liberty' },

  // Middleware - TIBCO
  tibco: { icon: 'Layers', color: '#E31837', label: 'TIBCO' },
  tibcoems: { icon: 'Layers', color: '#E31837', label: 'TIBCO EMS' },
  tibcobw: { icon: 'Workflow', color: '#E31837', label: 'TIBCO BW' },

  // Middleware - Oracle
  weblogic: { icon: 'Server', color: '#F80000', label: 'WebLogic' },
  tuxedo: { icon: 'Layers', color: '#F80000', label: 'Tuxedo' },

  // Message Queues
  rabbitmq: { icon: 'Layers', color: '#FF6600', label: 'RabbitMQ' },
  kafka: { icon: 'Layers', color: '#231F20', label: 'Kafka' },
  activemq: { icon: 'Layers', color: '#D5382F', label: 'ActiveMQ' },

  // Schedulers
  controlm: { icon: 'Calendar', color: '#E31837', label: 'Control-M' },
  autosys: { icon: 'Calendar', color: '#005B85', label: 'AutoSys' },
  dollaruniverse: { icon: 'Calendar', color: '#1E88E5', label: 'Dollar Universe' },
  tws: { icon: 'Calendar', color: '#054ADA', label: 'IBM TWS' },

  // File Transfer
  connectdirect: { icon: 'ArrowLeftRight', color: '#054ADA', label: 'Connect:Direct' },
  axway: { icon: 'ArrowLeftRight', color: '#7B1FA2', label: 'Axway CFT' },

  // Security
  cyberark: { icon: 'Shield', color: '#0070AD', label: 'CyberArk' },

  // Web Servers
  nginx: { icon: 'Globe', color: '#009639', label: 'Nginx' },
  apache: { icon: 'Globe', color: '#D22128', label: 'Apache' },
  iis: { icon: 'Globe', color: '#0078D4', label: 'IIS' },
  haproxy: { icon: 'Network', color: '#106DA9', label: 'HAProxy' },
  f5: { icon: 'Network', color: '#E4002B', label: 'F5 BIG-IP' },

  // App Servers
  tomcat: { icon: 'Server', color: '#F8DC75', label: 'Tomcat' },
  jboss: { icon: 'Server', color: '#CC0000', label: 'JBoss' },
  wildfly: { icon: 'Server', color: '#CC0000', label: 'WildFly' },

  // Languages/Runtimes
  nodejs: { icon: 'Server', color: '#339933', label: 'Node.js' },
  dotnet: { icon: 'Server', color: '#512BD4', label: '.NET' },

  // XComponent
  xcomponent: { icon: 'Puzzle', color: '#6366F1', label: 'XComponent' },

  // Infrastructure
  docker: { icon: 'Container', color: '#2496ED', label: 'Docker' },
  zookeeper: { icon: 'Folder', color: '#C73A63', label: 'ZooKeeper' },
};

// Layer colors for grouping in discovery
export const LAYER_COLORS: Record<string, string> = {
  'Database': '#1565C0',
  'Middleware': '#6A1B9A',
  'Application': '#2E7D32',
  'Access Points': '#E65100',
  'Scheduler': '#4E342E',
  'File Transfer': '#795548',
  'Security': '#0070AD',
  'Infrastructure': '#37474F',
};
