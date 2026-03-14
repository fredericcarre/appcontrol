import { useState } from 'react';
import { ChevronDown, ChevronRight, Copy, Check, Variable } from 'lucide-react';
import { cn } from '@/lib/utils';

interface EnvVarsDisplayProps {
  envVars: Record<string, string>;
  maxInitialShow?: number;
}

// Environment variable prefixes that are likely relevant for app configuration
const RELEVANT_PREFIXES = [
  'DB_', 'DATABASE_', 'POSTGRES', 'MYSQL', 'MONGO', 'REDIS',
  'MQ_', 'RABBIT', 'KAFKA', 'AMQP',
  'API_', 'SERVICE_', 'APP_', 'SERVER_',
  'PORT', 'HOST', 'URL', 'ENDPOINT',
  'LOG', 'DEBUG',
  'AWS_', 'AZURE_', 'GCP_', 'CLOUD_',
  'JAVA_', 'NODE_', 'PYTHON', 'RUBY',
  'HOME', 'USER', 'PATH',
];

// Prefixes to filter out (system/noise)
const FILTER_PREFIXES = [
  'DBUS_', 'XDG_', 'DISPLAY', 'TERM', 'SHELL', 'PWD', 'OLDPWD',
  'SHLVL', 'WINDOWID', 'COLORTERM', 'LS_COLORS', 'LESS',
  '_', 'SSH_', 'GPG_', 'GNOME_', 'GTK_', 'QT_',
];

function isRelevantEnvVar(key: string): boolean {
  const upper = key.toUpperCase();
  // Filter out system vars
  if (FILTER_PREFIXES.some((p) => upper.startsWith(p))) return false;
  // Include relevant vars
  return RELEVANT_PREFIXES.some((p) => upper.includes(p)) || upper.includes('=');
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <button
      onClick={handleCopy}
      className="p-0.5 rounded hover:bg-accent opacity-0 group-hover:opacity-100 transition-opacity"
      title="Copy"
    >
      {copied ? <Check className="h-3 w-3 text-emerald-500" /> : <Copy className="h-3 w-3 text-muted-foreground" />}
    </button>
  );
}

export function EnvVarsDisplay({ envVars, maxInitialShow = 5 }: EnvVarsDisplayProps) {
  const [expanded, setExpanded] = useState(false);

  // Sort and filter env vars
  const entries = Object.entries(envVars)
    .filter(([key]) => isRelevantEnvVar(key))
    .sort((a, b) => a[0].localeCompare(b[0]));

  if (entries.length === 0) return null;

  const displayEntries = expanded ? entries : entries.slice(0, maxInitialShow);
  const hasMore = entries.length > maxInitialShow;

  return (
    <div>
      <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
        <Variable className="h-3 w-3 text-violet-500" />
        ENV VARIABLES ({entries.length})
      </div>
      <div className="space-y-1 pl-2 border-l-2 border-border">
        {displayEntries.map(([key, value]) => (
          <div
            key={key}
            className="flex items-start gap-1 text-[11px] group"
          >
            <span className="text-violet-600 font-mono flex-shrink-0">{key}=</span>
            <span
              className="text-muted-foreground font-mono truncate flex-1"
              title={value}
            >
              {value.length > 50 ? `${value.substring(0, 50)}...` : value}
            </span>
            <CopyButton text={`${key}=${value}`} />
          </div>
        ))}
        {hasMore && (
          <button
            onClick={() => setExpanded(!expanded)}
            className={cn(
              'flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors',
              'mt-1 py-0.5'
            )}
          >
            {expanded ? (
              <>
                <ChevronDown className="h-3 w-3" />
                Show less
              </>
            ) : (
              <>
                <ChevronRight className="h-3 w-3" />
                Show {entries.length - maxInitialShow} more
              </>
            )}
          </button>
        )}
      </div>
    </div>
  );
}
