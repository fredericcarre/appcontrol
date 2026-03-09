import { X, Shield, FileText, ScrollText, Terminal, Network } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { COMPONENT_TYPE_ICONS, TECHNOLOGY_COLORS, type ComponentType } from '@/lib/colors';
import { useDiscoveryStore } from '@/stores/discovery';
import { useAgents } from '@/api/reports';

const COMPONENT_TYPES: ComponentType[] = ['database', 'middleware', 'appserver', 'webfront', 'service', 'batch', 'custom'];

const CONFIDENCE_LABELS: Record<string, { label: string; color: string }> = {
  high: { label: 'High', color: 'text-emerald-600 bg-emerald-50 border-emerald-200' },
  medium: { label: 'Medium', color: 'text-amber-600 bg-amber-50 border-amber-200' },
  low: { label: 'Low', color: 'text-slate-500 bg-slate-50 border-slate-200' },
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ServiceDetailPanel() {
  const {
    correlationResult,
    selectedServiceIndex,
    setSelectedServiceIndex,
    serviceEdits,
    updateServiceEdit,
    enabledServiceIndices,
    toggleServiceEnabled,
    getEffectiveName,
    getEffectiveType,
  } = useDiscoveryStore();

  const { data: agents } = useAgents();

  if (selectedServiceIndex === null || !correlationResult) return null;

  const service = correlationResult.services[selectedServiceIndex];
  if (!service) return null;

  // Find agent info for gateway display
  const agentInfo = agents?.find((a) => a.id === service.agent_id);

  const edits = serviceEdits.get(selectedServiceIndex);
  const effectiveName = getEffectiveName(selectedServiceIndex);
  const effectiveType = getEffectiveType(selectedServiceIndex) as ComponentType;
  const typeInfo = COMPONENT_TYPE_ICONS[effectiveType] || COMPONENT_TYPE_ICONS.service;
  const enabled = enabledServiceIndices.has(selectedServiceIndex);
  const cmdSuggestion = service.command_suggestion;
  const confInfo = CONFIDENCE_LABELS[cmdSuggestion?.confidence || 'low'] || CONFIDENCE_LABELS.low;

  return (
    <div className="w-[360px] border-l border-border bg-card h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 p-3 border-b border-border">
        <div
          className="w-3 h-3 rounded-full flex-shrink-0"
          style={{ backgroundColor: typeInfo.color }}
        />
        <input
          type="text"
          value={edits?.name ?? effectiveName}
          onChange={(e) => updateServiceEdit(selectedServiceIndex, { name: e.target.value })}
          className="font-semibold text-sm bg-transparent border-none outline-none flex-1 min-w-0 focus:ring-1 focus:ring-primary rounded px-1 -mx-1"
        />
        <button
          onClick={() => setSelectedServiceIndex(null)}
          className="p-1 rounded hover:bg-accent"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {/* Include toggle + hostname */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">{service.hostname}</span>
          {service.matched_service && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0">
              <Shield className="h-2.5 w-2.5 mr-0.5" />
              {service.matched_service}
            </Badge>
          )}
        </div>
        <Button
          size="sm"
          variant={enabled ? 'default' : 'outline'}
          className="h-6 text-[10px] px-2"
          onClick={() => toggleServiceEnabled(selectedServiceIndex)}
        >
          {enabled ? 'Included' : 'Excluded'}
        </Button>
      </div>

      {/* Tabs */}
      <Tabs defaultValue="info" className="flex-1 flex flex-col overflow-hidden">
        <TabsList className="mx-3 mt-2 h-8">
          <TabsTrigger value="info" className="text-xs h-6">Info</TabsTrigger>
          <TabsTrigger value="config" className="text-xs h-6">Config</TabsTrigger>
          <TabsTrigger value="logs" className="text-xs h-6">Logs</TabsTrigger>
          <TabsTrigger value="commands" className="text-xs h-6">Commands</TabsTrigger>
        </TabsList>

        <ScrollArea className="flex-1">
          {/* Info Tab */}
          <TabsContent value="info" className="px-3 pb-3 mt-0 space-y-3">
            <div className="space-y-2 pt-2">
              <div>
                <label className="text-[10px] text-muted-foreground uppercase tracking-wider">Process</label>
                <div className="text-xs font-mono">{service.process_name}</div>
              </div>
              <div>
                <label className="text-[10px] text-muted-foreground uppercase tracking-wider">Type</label>
                <select
                  value={edits?.componentType ?? effectiveType}
                  onChange={(e) => updateServiceEdit(selectedServiceIndex, { componentType: e.target.value })}
                  className="w-full text-xs rounded border border-input bg-background px-2 py-1 mt-0.5"
                >
                  {COMPONENT_TYPES.map((t) => (
                    <option key={t} value={t}>{t}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="text-[10px] text-muted-foreground uppercase tracking-wider">Ports</label>
                <div className="flex flex-wrap gap-1 mt-0.5">
                  {service.ports.map((p) => (
                    <Badge key={p} variant="secondary" className="font-mono text-[10px]">:{p}</Badge>
                  ))}
                  {service.ports.length === 0 && (
                    <span className="text-xs text-muted-foreground">None</span>
                  )}
                </div>
              </div>
              <div>
                <label className="text-[10px] text-muted-foreground uppercase tracking-wider">Gateway</label>
                {agentInfo?.gateway_name ? (
                  <div className="flex items-center gap-1.5 mt-0.5">
                    <Network className="h-3 w-3 text-muted-foreground" />
                    <span className="text-xs font-medium">{agentInfo.gateway_name}</span>
                    {agentInfo.gateway_zone && (
                      <span className="text-[10px] text-muted-foreground">({agentInfo.gateway_zone})</span>
                    )}
                    <div
                      className={`w-1.5 h-1.5 rounded-full ml-auto ${
                        agentInfo.gateway_connected ? 'bg-emerald-500' : 'bg-slate-400'
                      }`}
                      title={agentInfo.gateway_connected ? 'Gateway connected' : 'Gateway disconnected'}
                    />
                  </div>
                ) : (
                  <div className="text-[10px] font-mono text-muted-foreground truncate">
                    {service.agent_id}
                  </div>
                )}
              </div>
              <div>
                <label className="text-[10px] text-muted-foreground uppercase tracking-wider">Agent</label>
                <div className="flex items-center gap-1.5 mt-0.5">
                  <span className="text-xs">{service.hostname}</span>
                  {agentInfo && (
                    <div
                      className={`w-1.5 h-1.5 rounded-full ml-auto ${
                        agentInfo.connected ? 'bg-emerald-500' : 'bg-slate-400'
                      }`}
                      title={agentInfo.connected ? 'Agent connected' : 'Agent disconnected'}
                    />
                  )}
                </div>
              </div>
            </div>
          </TabsContent>

          {/* Config Tab */}
          <TabsContent value="config" className="px-3 pb-3 mt-0 space-y-2">
            {(!service.config_files || service.config_files.length === 0) ? (
              <div className="text-xs text-muted-foreground py-4 text-center">No config files detected</div>
            ) : (
              service.config_files.map((cf, i) => (
                <div key={i} className="border border-border rounded-md p-2">
                  <div className="flex items-center gap-1.5 mb-1">
                    <FileText className="h-3.5 w-3.5 text-blue-500" />
                    <span className="text-xs font-mono truncate" title={cf.path}>
                      {cf.path.split('/').pop()}
                    </span>
                  </div>
                  <div className="text-[10px] text-muted-foreground truncate mb-1">{cf.path}</div>
                  {cf.extracted_endpoints && cf.extracted_endpoints.length > 0 && (
                    <div className="space-y-1 mt-1.5 pt-1.5 border-t border-border">
                      {cf.extracted_endpoints.map((ep, j) => (
                        <div key={j} className="flex items-center gap-1 text-[10px]">
                          {ep.technology && (
                            <span
                              className="font-medium px-1 rounded"
                              style={{
                                color: TECHNOLOGY_COLORS[ep.technology] || TECHNOLOGY_COLORS.default,
                                backgroundColor: `${TECHNOLOGY_COLORS[ep.technology] || TECHNOLOGY_COLORS.default}15`,
                              }}
                            >
                              {ep.technology}
                            </span>
                          )}
                          <span className="text-muted-foreground truncate">{ep.key}</span>
                          <span className="text-muted-foreground">=</span>
                          <span className="font-mono truncate flex-1">{ep.value}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))
            )}
          </TabsContent>

          {/* Logs Tab */}
          <TabsContent value="logs" className="px-3 pb-3 mt-0 space-y-2">
            {(!service.log_files || service.log_files.length === 0) ? (
              <div className="text-xs text-muted-foreground py-4 text-center">No log files detected</div>
            ) : (
              service.log_files.map((lf, i) => (
                <div key={i} className="flex items-center gap-2 border border-border rounded-md p-2">
                  <ScrollText className="h-3.5 w-3.5 text-amber-500 flex-shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-mono truncate" title={lf.path}>
                      {lf.path.split('/').pop()}
                    </div>
                    <div className="text-[10px] text-muted-foreground">{lf.path}</div>
                  </div>
                  <span className="text-[10px] text-muted-foreground flex-shrink-0">
                    {formatBytes(lf.size_bytes)}
                  </span>
                </div>
              ))
            )}
          </TabsContent>

          {/* Commands Tab */}
          <TabsContent value="commands" className="px-3 pb-3 mt-0 space-y-3">
            {/* Confidence badge */}
            {cmdSuggestion && (
              <div className="flex items-center gap-2 pt-2">
                <Badge variant="outline" className={`text-[10px] ${confInfo.color}`}>
                  <Shield className="h-2.5 w-2.5 mr-0.5" />
                  {confInfo.label}
                </Badge>
                <span className="text-[10px] text-muted-foreground">via {cmdSuggestion.source}</span>
              </div>
            )}

            <Separator />

            {/* Command inputs */}
            {(['check', 'start', 'stop', 'restart'] as const).map((cmd) => {
              const fieldKey = `${cmd}Cmd` as 'checkCmd' | 'startCmd' | 'stopCmd' | 'restartCmd';
              const apiKey = `${cmd}_cmd` as 'check_cmd' | 'start_cmd' | 'stop_cmd' | 'restart_cmd';
              const value = edits?.[fieldKey] ?? cmdSuggestion?.[apiKey] ?? '';
              return (
                <div key={cmd}>
                  <label className="text-[10px] text-muted-foreground uppercase tracking-wider flex items-center gap-1">
                    <Terminal className="h-3 w-3" />
                    {cmd}
                  </label>
                  <input
                    type="text"
                    value={value}
                    onChange={(e) => updateServiceEdit(selectedServiceIndex, { [fieldKey]: e.target.value })}
                    placeholder={`e.g. systemctl ${cmd} myservice`}
                    className="w-full text-xs font-mono rounded border border-input bg-background px-2 py-1.5 mt-0.5 focus:ring-1 focus:ring-primary outline-none"
                  />
                </div>
              );
            })}
          </TabsContent>
        </ScrollArea>
      </Tabs>
    </div>
  );
}
