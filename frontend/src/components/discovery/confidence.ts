import type { CorrelatedService } from '@/api/discovery';
import type { ServiceConfidence } from './TopologyMap.types';

// System processes that should be filtered out by default
const SYSTEM_PROCESSES = new Set([
  'sshd',
  'systemd',
  'init',
  'cron',
  'crond',
  'rsyslogd',
  'syslogd',
  'klogd',
  'dbus-daemon',
  'polkitd',
  'firewalld',
  'iptables',
  'containerd',
  'dockerd',
  'kubelet',
  'kube-proxy',
  'etcd',
  'auditd',
  'chronyd',
  'ntpd',
  'login',
  'getty',
  'agetty',
  'sshd-keygen',
  'bash',
  'sh',
  'zsh',
]);

// Well-known ports that indicate a specific technology
const KNOWN_PORTS: Record<number, string> = {
  80: 'http',
  443: 'https',
  8080: 'http-alt',
  8443: 'https-alt',
  3000: 'nodejs',
  3306: 'mysql',
  5432: 'postgresql',
  5433: 'postgresql',
  1433: 'mssql',
  1521: 'oracle',
  27017: 'mongodb',
  27018: 'mongodb',
  6379: 'redis',
  6380: 'redis',
  9200: 'elasticsearch',
  9300: 'elasticsearch',
  5672: 'rabbitmq',
  15672: 'rabbitmq-mgmt',
  9092: 'kafka',
  2181: 'zookeeper',
  11211: 'memcached',
  1414: 'ibmmq',
  1883: 'mqtt',
  8883: 'mqtt-tls',
  5044: 'logstash',
  9600: 'logstash',
  5601: 'kibana',
  8500: 'consul',
  4369: 'epmd',
};

// Process names that likely indicate a specific service
const LIKELY_PROCESSES: Record<string, string> = {
  java: 'java-app',
  python: 'python-app',
  python3: 'python-app',
  node: 'nodejs-app',
  ruby: 'ruby-app',
  perl: 'perl-app',
  php: 'php-app',
  'php-fpm': 'php-fpm',
  go: 'go-app',
  dotnet: 'dotnet-app',
  nginx: 'nginx',
  httpd: 'apache',
  apache2: 'apache',
  tomcat: 'tomcat',
  catalina: 'tomcat',
  wildfly: 'wildfly',
  jboss: 'jboss',
  weblogic: 'weblogic',
  websphere: 'websphere',
};

/**
 * Classifies a service's confidence level based on available information.
 *
 * @param service - The correlated service to classify
 * @returns ServiceConfidence - 'recognized', 'likely', 'unknown', or 'system'
 */
export function classifyConfidence(service: CorrelatedService): ServiceConfidence {
  const processName = service.process_name?.toLowerCase() || '';

  // Check if it's a system process
  if (SYSTEM_PROCESSES.has(processName)) {
    return 'system';
  }

  // Check if technology_hint is present (highest confidence)
  if (service.technology_hint) {
    return 'recognized';
  }

  // Check if command suggestion has high confidence
  if (service.command_suggestion?.confidence === 'high') {
    return 'recognized';
  }

  // Check if any port is a well-known port
  const hasKnownPort = service.ports.some((port) => port in KNOWN_PORTS);
  if (hasKnownPort) {
    return 'likely';
  }

  // Check if process name is in likely processes list
  if (processName in LIKELY_PROCESSES) {
    return 'likely';
  }

  // Check if command suggestion exists with medium confidence
  if (service.command_suggestion?.confidence === 'medium') {
    return 'likely';
  }

  // Default to unknown
  return 'unknown';
}

/**
 * Get display info for a confidence level
 */
export function getConfidenceInfo(confidence: ServiceConfidence): {
  label: string;
  color: string;
  bgColor: string;
  borderColor: string;
  description: string;
} {
  switch (confidence) {
    case 'recognized':
      return {
        label: 'Recognized',
        color: 'text-emerald-700',
        bgColor: 'bg-emerald-50',
        borderColor: 'border-emerald-300',
        description: 'Technology identified with high confidence',
      };
    case 'likely':
      return {
        label: 'Likely',
        color: 'text-amber-700',
        bgColor: 'bg-amber-50',
        borderColor: 'border-amber-300',
        description: 'Service detected via known ports or patterns',
      };
    case 'unknown':
      return {
        label: 'Unknown',
        color: 'text-slate-600',
        bgColor: 'bg-slate-50',
        borderColor: 'border-slate-300',
        description: 'Generic process, needs manual identification',
      };
    case 'system':
      return {
        label: 'System',
        color: 'text-slate-400',
        bgColor: 'bg-slate-100',
        borderColor: 'border-slate-200',
        description: 'System process (sshd, cron, etc.)',
      };
  }
}

/**
 * Get counts of services by confidence level
 */
export function getConfidenceCounts(
  services: CorrelatedService[]
): Record<ServiceConfidence, number> {
  const counts: Record<ServiceConfidence, number> = {
    recognized: 0,
    likely: 0,
    unknown: 0,
    system: 0,
  };

  for (const service of services) {
    const confidence = classifyConfidence(service);
    counts[confidence]++;
  }

  return counts;
}
