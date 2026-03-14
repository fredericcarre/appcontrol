import { useState } from 'react';
import { X, FileText, ScrollText, Terminal, Eye, Copy, Check, ExternalLink, Loader2, ArrowRight, User, Trash2, RotateCcw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { useDiscoveryStore } from '@/stores/discovery';
import { useReadFileContent } from '@/api/discovery';
import { EnvVarsDisplay } from './EnvVarsDisplay';
import { CustomActionEditor } from './CustomActionEditor';
import { BatchJobLinker } from './BatchJobLinker';
import { classifyConfidence, getConfidenceInfo } from './confidence';

// Common port to technology mapping
const PORT_HINTS: Record<number, string> = {
  5672: 'RabbitMQ',
  15672: 'RabbitMQ Mgmt',
  5432: 'PostgreSQL',
  3306: 'MySQL',
  1521: 'Oracle',
  1433: 'SQL Server',
  27017: 'MongoDB',
  6379: 'Redis',
  9200: 'Elasticsearch',
  9092: 'Kafka',
  2181: 'ZooKeeper',
  8080: 'HTTP',
  8443: 'HTTPS',
  443: 'HTTPS',
  80: 'HTTP',
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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
      className="p-0.5 rounded hover:bg-accent"
      title="Copy"
    >
      {copied ? <Check className="h-3 w-3 text-emerald-500" /> : <Copy className="h-3 w-3 text-muted-foreground" />}
    </button>
  );
}

interface FileContentDialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  path: string;
  agentId: string;
  isLog?: boolean;
}

function FileContentDialog({ open, onClose, title, path, agentId, isLog }: FileContentDialogProps) {
  const [content, setContent] = useState<string | null>(null);
  const readFile = useReadFileContent();

  const loadContent = () => {
    setContent(null);
    readFile.mutate(
      { agent_id: agentId, path, tail_lines: isLog ? 100 : undefined },
      {
        onSuccess: (data) => {
          setContent(`Request sent (ID: ${data.request_id})\n\nThe file content will appear in the command output panel.\nPath: ${path}`);
        },
        onError: (err) => {
          setContent(`Error: ${err.message}`);
        },
      }
    );
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-3xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-sm">
            {isLog ? <ScrollText className="h-4 w-4 text-amber-500" /> : <FileText className="h-4 w-4 text-blue-500" />}
            {title}
          </DialogTitle>
        </DialogHeader>
        <div className="text-[10px] text-muted-foreground font-mono truncate mb-2">{path}</div>
        {content === null ? (
          <div className="flex-1 flex flex-col items-center justify-center gap-4 py-8">
            <p className="text-sm text-muted-foreground">Click to load file content from agent</p>
            <Button onClick={loadContent} disabled={readFile.isPending}>
              {readFile.isPending && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              {isLog ? 'Load Last 100 Lines' : 'Load Content'}
            </Button>
          </div>
        ) : (
          <ScrollArea className="flex-1 border rounded-md">
            <pre className="p-3 text-xs font-mono whitespace-pre-wrap break-all">{content}</pre>
          </ScrollArea>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function ServiceDetailPanel() {
  const {
    correlationResult,
    selectedServiceIndex,
    setSelectedServiceIndex,
    ignoreDependency,
    restoreDependency,
    isDependencyIgnored,
    removeManualDependency,
    manualDependencies,
  } = useDiscoveryStore();

  const [fileDialog, setFileDialog] = useState<{ path: string; title: string; isLog: boolean } | null>(null);

  if (selectedServiceIndex === null || !correlationResult) return null;

  const service = correlationResult.services[selectedServiceIndex];
  if (!service) return null;

  const cmdSuggestion = service.command_suggestion;
  const confidence = classifyConfidence(service);
  const confidenceInfo = getConfidenceInfo(confidence);

  // Find connections from this service
  const outgoingDeps = correlationResult.dependencies.filter(
    (d) => d.from_service_index === selectedServiceIndex
  );

  // Find connections TO this service
  const incomingDeps = correlationResult.dependencies.filter(
    (d) => d.to_service_index === selectedServiceIndex
  );

  const getServiceName = (idx: number) => {
    const svc = correlationResult.services[idx];
    return svc?.technology_hint?.display_name || svc?.process_name || `Service ${idx}`;
  };

  return (
    <div className="w-[380px] border-l border-border bg-card h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-border bg-muted/30">
        <div className="flex items-center gap-2 flex-1 min-w-0">
          <span className="font-semibold text-sm truncate">
            {service.technology_hint?.display_name || service.process_name}
          </span>
          <Badge
            variant="outline"
            className={`text-[9px] h-4 px-1 ${confidenceInfo.color} ${confidenceInfo.borderColor}`}
          >
            {confidenceInfo.label}
          </Badge>
        </div>
        <button
          onClick={() => setSelectedServiceIndex(null)}
          className="p-1 rounded hover:bg-accent"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-3 space-y-4">
          {/* Identity Section */}
          <div>
            <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
              IDENTITY
            </div>
            <div className="space-y-1.5 text-xs pl-2 border-l-2 border-border">
              <div className="flex">
                <span className="text-muted-foreground w-16">Process:</span>
                <span className="font-mono">{service.process_name}</span>
              </div>
              <div className="flex">
                <span className="text-muted-foreground w-16">Host:</span>
                <span className="font-medium">{service.hostname}</span>
              </div>
              {service.user && (
                <div className="flex items-center">
                  <span className="text-muted-foreground w-16">User:</span>
                  <span className="font-mono flex items-center gap-1">
                    <User className="h-3 w-3 text-muted-foreground" />
                    {service.user}
                  </span>
                </div>
              )}
              <div className="flex">
                <span className="text-muted-foreground w-16">Ports:</span>
                <span className="font-mono">
                  {service.ports.length > 0 ? service.ports.map(p => `:${p}`).join(', ') : '-'}
                </span>
              </div>
            </div>
          </div>

          {/* Commands Section */}
          <div>
            <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
              <Terminal className="h-3 w-3" />
              COMMANDS
            </div>
            <div className="space-y-1.5 pl-2 border-l-2 border-border">
              {[
                { label: 'Check', value: cmdSuggestion?.check_cmd, color: 'text-emerald-600' },
                { label: 'Start', value: cmdSuggestion?.start_cmd, color: 'text-blue-600' },
                { label: 'Stop', value: cmdSuggestion?.stop_cmd, color: 'text-red-600' },
                { label: 'Logs', value: cmdSuggestion?.logs_cmd, color: 'text-amber-600' },
                { label: 'Version', value: cmdSuggestion?.version_cmd, color: 'text-violet-600' },
              ].map(({ label, value, color }) => (
                value ? (
                  <div key={label} className="flex items-start gap-1 text-[11px]">
                    <span className={`${color} w-12 flex-shrink-0`}>{label}:</span>
                    <code className="text-muted-foreground font-mono truncate flex-1" title={value}>
                      {value}
                    </code>
                    <CopyButton text={value} />
                  </div>
                ) : null
              ))}
              {!cmdSuggestion?.check_cmd && !cmdSuggestion?.start_cmd && !cmdSuggestion?.stop_cmd && (
                <div className="text-[11px] text-muted-foreground italic">No commands detected</div>
              )}
            </div>
          </div>

          {/* Config Files Section */}
          {service.config_files && service.config_files.length > 0 && (
            <div>
              <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
                <FileText className="h-3 w-3 text-blue-500" />
                CONFIG FILES ({service.config_files.length})
              </div>
              <div className="space-y-1 pl-2 border-l-2 border-border">
                {service.config_files.map((cf, i) => (
                  <div key={i} className="flex items-center gap-1 text-[11px]">
                    <span className="font-mono truncate flex-1" title={cf.path}>
                      {cf.path.split(/[/\\]/).pop()}
                    </span>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-5 w-5"
                      onClick={() => setFileDialog({ path: cf.path, title: cf.path.split(/[/\\]/).pop() || cf.path, isLog: false })}
                      title="View content"
                    >
                      <Eye className="h-3 w-3" />
                    </Button>
                    <CopyButton text={cf.path} />
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Log Files Section */}
          {service.log_files && service.log_files.length > 0 && (
            <div>
              <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
                <ScrollText className="h-3 w-3 text-amber-500" />
                LOG FILES ({service.log_files.length})
              </div>
              <div className="space-y-1 pl-2 border-l-2 border-border">
                {service.log_files.map((lf, i) => (
                  <div key={i} className="flex items-center gap-1 text-[11px]">
                    <span className="font-mono truncate flex-1" title={lf.path}>
                      {lf.path.split(/[/\\]/).pop()}
                    </span>
                    <span className="text-[9px] text-muted-foreground">
                      {formatBytes(lf.size_bytes)}
                    </span>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-5 w-5"
                      onClick={() => setFileDialog({ path: lf.path, title: lf.path.split(/[/\\]/).pop() || lf.path, isLog: true })}
                      title="View last 100 lines"
                    >
                      <Eye className="h-3 w-3" />
                    </Button>
                    <CopyButton text={lf.path} />
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Custom Actions Section */}
          <CustomActionEditor serviceIndex={selectedServiceIndex} />

          {/* Env Vars Section */}
          {service.env_vars && Object.keys(service.env_vars).length > 0 && (
            <EnvVarsDisplay envVars={service.env_vars} />
          )}

          {/* Batch Jobs Section */}
          <BatchJobLinker serviceIndex={selectedServiceIndex} />

          {/* Connections Section - Outgoing */}
          {outgoingDeps.length > 0 && (
            <div>
              <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
                <ExternalLink className="h-3 w-3" />
                OUTGOING ({outgoingDeps.length})
              </div>
              <div className="space-y-1 pl-2 border-l-2 border-border">
                {outgoingDeps.map((dep, i) => {
                  const isIgnored = dep.from_service_index !== null && isDependencyIgnored(dep.from_service_index, dep.to_service_index);
                  return (
                    <div key={i} className={`flex items-center gap-1 text-[11px] ${isIgnored ? 'opacity-50' : ''}`}>
                      <ArrowRight className="h-3 w-3 text-emerald-500 flex-shrink-0" />
                      <span className="text-muted-foreground truncate flex-1">
                        {getServiceName(dep.to_service_index)}
                      </span>
                      {PORT_HINTS[dep.remote_port] && (
                        <Badge variant="secondary" className="text-[9px] h-4 px-1">
                          {PORT_HINTS[dep.remote_port]}
                        </Badge>
                      )}
                      <span className="font-mono text-[10px] text-muted-foreground">
                        :{dep.remote_port}
                      </span>
                      {dep.from_service_index !== null && (
                        isIgnored ? (
                          <Button
                            size="icon"
                            variant="ghost"
                            className="h-5 w-5 text-muted-foreground hover:text-emerald-500"
                            onClick={() => restoreDependency(dep.from_service_index!, dep.to_service_index)}
                            title="Restore dependency"
                          >
                            <RotateCcw className="h-3 w-3" />
                          </Button>
                        ) : (
                          <Button
                            size="icon"
                            variant="ghost"
                            className="h-5 w-5 text-muted-foreground hover:text-red-500"
                            onClick={() => ignoreDependency(dep.from_service_index!, dep.to_service_index)}
                            title="Ignore dependency"
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        )
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Connections Section - Incoming */}
          {incomingDeps.length > 0 && (
            <div>
              <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
                <ArrowRight className="h-3 w-3 rotate-180" />
                INCOMING ({incomingDeps.length})
              </div>
              <div className="space-y-1 pl-2 border-l-2 border-border">
                {incomingDeps.map((dep, i) => {
                  const isIgnored = dep.from_service_index !== null && isDependencyIgnored(dep.from_service_index, dep.to_service_index);
                  return (
                    <div key={i} className={`flex items-center gap-1 text-[11px] ${isIgnored ? 'opacity-50' : ''}`}>
                      <ArrowRight className="h-3 w-3 text-blue-500 rotate-180 flex-shrink-0" />
                      <span className="text-muted-foreground truncate flex-1">
                        {dep.from_service_index !== null ? getServiceName(dep.from_service_index) : dep.from_process}
                      </span>
                      {PORT_HINTS[dep.remote_port] && (
                        <Badge variant="secondary" className="text-[9px] h-4 px-1">
                          {PORT_HINTS[dep.remote_port]}
                        </Badge>
                      )}
                      <span className="font-mono text-[10px] text-muted-foreground">
                        :{dep.remote_port}
                      </span>
                      {dep.from_service_index !== null && (
                        isIgnored ? (
                          <Button
                            size="icon"
                            variant="ghost"
                            className="h-5 w-5 text-muted-foreground hover:text-emerald-500"
                            onClick={() => restoreDependency(dep.from_service_index!, dep.to_service_index)}
                            title="Restore dependency"
                          >
                            <RotateCcw className="h-3 w-3" />
                          </Button>
                        ) : (
                          <Button
                            size="icon"
                            variant="ghost"
                            className="h-5 w-5 text-muted-foreground hover:text-red-500"
                            onClick={() => ignoreDependency(dep.from_service_index!, dep.to_service_index)}
                            title="Ignore dependency"
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        )
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Manual Dependencies for this service */}
          {(() => {
            const outgoingManual = manualDependencies.filter((d) => d.from === selectedServiceIndex);
            const incomingManual = manualDependencies.filter((d) => d.to === selectedServiceIndex);
            const hasManual = outgoingManual.length > 0 || incomingManual.length > 0;

            if (!hasManual) return null;

            return (
              <div>
                <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-2 flex items-center gap-1">
                  <ExternalLink className="h-3 w-3 text-emerald-500" />
                  MANUAL LINKS ({outgoingManual.length + incomingManual.length})
                </div>
                <div className="space-y-1 pl-2 border-l-2 border-emerald-300">
                  {outgoingManual.map((md, i) => (
                    <div key={`out-${i}`} className="flex items-center gap-1 text-[11px]">
                      <ArrowRight className="h-3 w-3 text-emerald-500 flex-shrink-0" />
                      <span className="text-muted-foreground truncate flex-1">
                        {getServiceName(md.to)}
                      </span>
                      <Badge variant="outline" className="text-[9px] h-4 px-1 border-emerald-300 text-emerald-600">
                        manual
                      </Badge>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-5 w-5 text-muted-foreground hover:text-red-500"
                        onClick={() => removeManualDependency(md.from, md.to)}
                        title="Remove manual dependency"
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  ))}
                  {incomingManual.map((md, i) => (
                    <div key={`in-${i}`} className="flex items-center gap-1 text-[11px]">
                      <ArrowRight className="h-3 w-3 text-emerald-500 rotate-180 flex-shrink-0" />
                      <span className="text-muted-foreground truncate flex-1">
                        {getServiceName(md.from)}
                      </span>
                      <Badge variant="outline" className="text-[9px] h-4 px-1 border-emerald-300 text-emerald-600">
                        manual
                      </Badge>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-5 w-5 text-muted-foreground hover:text-red-500"
                        onClick={() => removeManualDependency(md.from, md.to)}
                        title="Remove manual dependency"
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  ))}
                </div>
              </div>
            );
          })()}
        </div>
      </ScrollArea>

      {/* File content dialog */}
      {fileDialog && (
        <FileContentDialog
          open={true}
          onClose={() => setFileDialog(null)}
          title={fileDialog.title}
          path={fileDialog.path}
          agentId={service.agent_id}
          isLog={fileDialog.isLog}
        />
      )}
    </div>
  );
}
