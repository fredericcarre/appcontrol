import { useMemo, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Server, Cog, Cloud, Clock, ChevronDown, ChevronRight, Search,
  PanelLeftClose, PanelLeft, Box, ArrowRight, Settings2, Plus, Check,
  ExternalLink,
} from 'lucide-react';
import { COMPONENT_TYPE_ICONS, type ComponentType } from '@/lib/colors';
import { useDiscoveryStore } from '@/stores/discovery';
import { useApps, type Application } from '@/api/apps';
import { cn } from '@/lib/utils';
import type { SystemService, DiscoveredScheduledJob } from '@/api/discovery';

// Component to display a system service with add button
function SystemServiceRow({ service }: { service: SystemService }) {
  const [added, setAdded] = useState(false);
  const addSystemServiceAsComponent = useDiscoveryStore((s) => s.addSystemServiceAsComponent);

  const handleAdd = () => {
    addSystemServiceAsComponent(service);
    setAdded(true);
  };

  const statusColor = service.status === 'running'
    ? 'bg-emerald-500'
    : service.status === 'stopped'
      ? 'bg-slate-400'
      : 'bg-amber-500';

  return (
    <div className="flex items-center gap-2 px-2 py-1 text-left group hover:bg-accent rounded">
      <div className={cn('w-2 h-2 rounded-full flex-shrink-0', statusColor)} />
      <span className="text-xs truncate flex-1" title={service.display_name}>
        {service.display_name || service.name}
      </span>
      <span className="text-[9px] text-muted-foreground capitalize">{service.status}</span>
      {added ? (
        <Check className="h-3.5 w-3.5 text-emerald-500" />
      ) : (
        <Button
          size="icon"
          variant="ghost"
          className="h-5 w-5 opacity-0 group-hover:opacity-100"
          onClick={handleAdd}
          title="Add as component"
        >
          <Plus className="h-3 w-3" />
        </Button>
      )}
    </div>
  );
}

// Component to display an existing app with add button
function ExistingAppRow({ app }: { app: Application }) {
  const [added, setAdded] = useState(false);
  const addExistingAppAsComponent = useDiscoveryStore((s) => s.addExistingAppAsComponent);

  const handleAdd = () => {
    addExistingAppAsComponent(app);
    setAdded(true);
  };

  // Weather/status indicator color
  const weatherColor = app.weather === 'sunny'
    ? 'bg-emerald-500'
    : app.weather === 'cloudy'
      ? 'bg-amber-500'
      : app.weather === 'stormy'
        ? 'bg-red-500'
        : 'bg-slate-400';

  const statusText = `${app.running_count}/${app.component_count}`;

  return (
    <div className="flex items-center gap-2 px-2 py-1 text-left group hover:bg-accent rounded">
      <div className={cn('w-2 h-2 rounded-full flex-shrink-0', weatherColor)} />
      <Box className="h-3 w-3 text-blue-400 flex-shrink-0" />
      <span className="text-xs truncate flex-1" title={app.name}>
        {app.name}
      </span>
      <Badge variant="outline" className="text-[9px] px-1 py-0 h-4">
        {statusText}
      </Badge>
      {added ? (
        <Check className="h-3.5 w-3.5 text-emerald-500" />
      ) : (
        <>
          <a
            href={`/apps/${app.id}`}
            target="_blank"
            rel="noopener noreferrer"
            className="opacity-0 group-hover:opacity-100"
            onClick={(e) => e.stopPropagation()}
            title="Open app"
          >
            <ExternalLink className="h-3 w-3 text-muted-foreground hover:text-foreground" />
          </a>
          <Button
            size="icon"
            variant="ghost"
            className="h-5 w-5 opacity-0 group-hover:opacity-100"
            onClick={handleAdd}
            title="Add as component (synthetic view)"
          >
            <Plus className="h-3 w-3" />
          </Button>
        </>
      )}
    </div>
  );
}

// Component to display a scheduled job with add button
function ScheduledJobRow({ job, index }: { job: DiscoveredScheduledJob; index: number }) {
  const { enabledBatchJobIndices, toggleBatchJobEnabled } = useDiscoveryStore();
  const isOnMap = enabledBatchJobIndices.has(index);
  const isEnabled = job.enabled !== false;

  return (
    <div className="flex items-center gap-2 px-2 py-1 text-left group hover:bg-accent rounded">
      <Clock className={cn('h-3 w-3 flex-shrink-0', isEnabled ? 'text-amber-500' : 'text-slate-400')} />
      <span className="text-xs truncate flex-1" title={job.command}>
        {job.name}
      </span>
      <span className="text-[9px] text-muted-foreground">{job.source}</span>
      {isOnMap ? (
        <Button
          size="icon"
          variant="ghost"
          className="h-5 w-5"
          onClick={() => toggleBatchJobEnabled(index)}
          title="Remove from map"
        >
          <Check className="h-3.5 w-3.5 text-emerald-500" />
        </Button>
      ) : (
        <Button
          size="icon"
          variant="ghost"
          className="h-5 w-5 opacity-0 group-hover:opacity-100"
          onClick={() => toggleBatchJobEnabled(index)}
          title="Add to map"
        >
          <Plus className="h-3 w-3" />
        </Button>
      )}
    </div>
  );
}

// Component to display an external connection with add button
function ExternalConnectionRow({ addr, port, index }: { addr: string; port: number; index: number }) {
  const { enabledExternalIndices, toggleExternalEnabled } = useDiscoveryStore();
  const isOnMap = enabledExternalIndices.has(index);

  return (
    <div className="flex items-center gap-2 px-2 py-1 text-left group hover:bg-accent rounded">
      <Cloud className="h-3 w-3 text-slate-400 flex-shrink-0" />
      <span className="text-xs truncate flex-1">{addr}</span>
      <span className="text-[9px] font-mono text-muted-foreground">:{port}</span>
      {isOnMap ? (
        <Button
          size="icon"
          variant="ghost"
          className="h-5 w-5"
          onClick={() => toggleExternalEnabled(index)}
          title="Remove from map"
        >
          <Check className="h-3.5 w-3.5 text-emerald-500" />
        </Button>
      ) : (
        <Button
          size="icon"
          variant="ghost"
          className="h-5 w-5 opacity-0 group-hover:opacity-100"
          onClick={() => toggleExternalEnabled(index)}
          title="Add to map"
        >
          <Plus className="h-3 w-3" />
        </Button>
      )}
    </div>
  );
}

export function LayerSidebar() {
  const {
    correlationResult,
    enabledServiceIndices,
    getEffectiveName,
    getEffectiveType,
    setSelectedServiceIndex,
    setHighlightedServiceIndex,
    enabledBatchJobIndices,
    enabledExternalIndices,
    batchJobsExpanded,
    setBatchJobsExpanded,
    externalsExpanded,
    setExternalsExpanded,
    searchQuery,
    setSearchQuery,
  } = useDiscoveryStore();

  const [collapsed, setCollapsed] = useState(false);
  const [hostsOpen, setHostsOpen] = useState(true);
  const [servicesOpen, setServicesOpen] = useState(true);
  const [appsOpen, setAppsOpen] = useState(true);
  const [systemServicesOpen, setSystemServicesOpen] = useState(false);

  const { data: existingApps } = useApps();
  const services = useMemo(() => correlationResult?.services || [], [correlationResult]);
  const unresolved = useMemo(() => correlationResult?.unresolved_connections || [], [correlationResult]);
  const scheduledJobs = useMemo(() => correlationResult?.scheduled_jobs || [], [correlationResult]);
  const systemServices = useMemo(() => correlationResult?.system_services || [], [correlationResult]);

  // Group services by hostname
  const hostGroups = useMemo(() => {
    const map = new Map<string, number[]>();
    services.forEach((s, i) => {
      const list = map.get(s.hostname) || [];
      list.push(i);
      map.set(s.hostname, list);
    });
    return map;
  }, [services]);

  // Filter by search
  const lowerQuery = searchQuery.toLowerCase();
  const matchesSearch = (text: string) => !lowerQuery || text.toLowerCase().includes(lowerQuery);

  // Unique external targets
  const externalTargets = useMemo(() => {
    const seen = new Set<string>();
    return unresolved.filter((c) => {
      const key = `${c.remote_addr}:${c.remote_port}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [unresolved]);

  if (collapsed) {
    return (
      <div className="w-12 border-r border-border bg-card flex flex-col items-center py-3 gap-3">
        <button onClick={() => setCollapsed(false)} className="p-1.5 rounded-md hover:bg-accent" title="Expand sidebar">
          <PanelLeft className="h-4 w-4 text-muted-foreground" />
        </button>
        <div className="w-6 border-t border-border" />
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <span title={`${hostGroups.size} hosts`}><Server className="h-4 w-4" /></span>
          <span className="text-[10px]">{hostGroups.size}</span>
          <span title={`${services.length} services`}><Cog className="h-4 w-4" /></span>
          <span className="text-[10px]">{services.length}</span>
          {systemServices.length > 0 && (
            <>
              <span title={`${systemServices.length} system services`}><Settings2 className="h-4 w-4" /></span>
              <span className="text-[10px]">{systemServices.length}</span>
            </>
          )}
          {scheduledJobs.length > 0 && (
            <>
              <span title={`${scheduledJobs.length} jobs`}><Clock className="h-4 w-4" /></span>
              <span className="text-[10px]">{scheduledJobs.length}</span>
            </>
          )}
          {externalTargets.length > 0 && (
            <>
              <span title={`${externalTargets.length} external`}><Cloud className="h-4 w-4" /></span>
              <span className="text-[10px]">{externalTargets.length}</span>
            </>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="w-[260px] border-r border-border bg-card flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Topology</span>
        <button onClick={() => setCollapsed(true)} className="p-1 rounded hover:bg-accent" title="Collapse sidebar">
          <PanelLeftClose className="h-4 w-4 text-muted-foreground" />
        </button>
      </div>

      {/* Search */}
      <div className="px-3 py-2 border-b border-border">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Filter..."
            className="h-7 text-xs pl-7"
          />
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-2 space-y-1">
          {/* HOSTS Section */}
          <button
            onClick={() => setHostsOpen(!hostsOpen)}
            className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
          >
            {hostsOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            <Server className="h-3.5 w-3.5 text-slate-500" />
            <span className="text-xs font-semibold flex-1">HOSTS</span>
            <Badge variant="secondary" className="text-[10px] px-1.5 py-0">{hostGroups.size}</Badge>
          </button>
          {hostsOpen && (
            <div className="pl-4 space-y-0.5">
              {[...hostGroups.entries()]
                .filter(([hostname]) => matchesSearch(hostname))
                .map(([hostname, indices]) => (
                  <button
                    key={hostname}
                    className="flex items-center gap-2 w-full px-2 py-1 rounded hover:bg-accent text-left group"
                    onClick={() => {
                      // Select first service of this host
                      if (indices.length > 0) setSelectedServiceIndex(indices[0]);
                    }}
                  >
                    <div className="w-2 h-2 rounded-full bg-emerald-500 flex-shrink-0" />
                    <span className="text-xs truncate flex-1">{hostname}</span>
                    <span className="text-[10px] text-muted-foreground">{indices.length}</span>
                  </button>
                ))}
            </div>
          )}

          {/* SERVICES Section */}
          <button
            onClick={() => setServicesOpen(!servicesOpen)}
            className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
          >
            {servicesOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            <Cog className="h-3.5 w-3.5 text-slate-500" />
            <span className="text-xs font-semibold flex-1">SERVICES</span>
            <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
              {enabledServiceIndices.size}/{services.length}
            </Badge>
          </button>
          {servicesOpen && (
            <div className="pl-4 space-y-0.5">
              {services
                .map((s, i) => ({ s, i }))
                .filter(({ s }) =>
                  matchesSearch(s.suggested_name) ||
                  matchesSearch(s.process_name) ||
                  matchesSearch(s.hostname)
                )
                .map(({ s, i }) => {
                  const ct = getEffectiveType(i) as ComponentType;
                  const typeInfo = COMPONENT_TYPE_ICONS[ct] || COMPONENT_TYPE_ICONS.service;
                  return (
                    <button
                      key={i}
                      className={cn(
                        'flex items-center gap-2 w-full px-2 py-1 rounded hover:bg-accent text-left group',
                        !enabledServiceIndices.has(i) && 'opacity-40',
                      )}
                      onClick={() => setSelectedServiceIndex(i)}
                      onMouseEnter={() => setHighlightedServiceIndex(i)}
                      onMouseLeave={() => setHighlightedServiceIndex(null)}
                    >
                      <div
                        className="w-2 h-2 rounded-full flex-shrink-0"
                        style={{ backgroundColor: typeInfo.color }}
                      />
                      <span className="text-xs truncate flex-1">{getEffectiveName(i)}</span>
                      {s.ports.length > 0 && (
                        <span className="text-[9px] font-mono text-muted-foreground">
                          :{s.ports[0]}
                        </span>
                      )}
                    </button>
                  );
                })}
            </div>
          )}

          {/* EXISTING APPS Section */}
          {existingApps && existingApps.length > 0 && (
            <>
              <button
                onClick={() => setAppsOpen(!appsOpen)}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                {appsOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <Box className="h-3.5 w-3.5 text-blue-500" />
                <span className="text-xs font-semibold flex-1">EXISTING APPS</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">{existingApps.length}</Badge>
              </button>
              {appsOpen && (
                <div className="pl-4 space-y-0.5">
                  <div className="px-2 py-1 text-[10px] text-muted-foreground border-b border-dashed mb-1">
                    Click + to add as synthetic component
                  </div>
                  {existingApps
                    .filter((app) => matchesSearch(app.name))
                    .slice(0, 15)
                    .map((app) => (
                      <ExistingAppRow key={app.id} app={app} />
                    ))}
                  {existingApps.length > 15 && (
                    <div className="px-2 py-1 text-[10px] text-muted-foreground">
                      +{existingApps.length - 15} more apps
                    </div>
                  )}
                </div>
              )}
            </>
          )}

          {/* SYSTEM SERVICES Section (Windows Services / systemd) */}
          {systemServices.length > 0 && (
            <>
              <button
                onClick={() => setSystemServicesOpen(!systemServicesOpen)}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                {systemServicesOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <Settings2 className="h-3.5 w-3.5 text-blue-600" />
                <span className="text-xs font-semibold flex-1">SYSTEM SERVICES</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">{systemServices.length}</Badge>
              </button>
              {systemServicesOpen && (
                <div className="pl-4 space-y-0.5">
                  <div className="px-2 py-1 text-[10px] text-muted-foreground border-b border-dashed mb-1">
                    Click + to add as component
                  </div>
                  {systemServices
                    .filter((svc) => matchesSearch(svc.name) || matchesSearch(svc.display_name))
                    .map((svc, i) => (
                      <SystemServiceRow key={i} service={svc} />
                    ))}
                </div>
              )}
            </>
          )}

          {/* BATCH JOBS Section */}
          {scheduledJobs.length > 0 && (
            <>
              <button
                onClick={() => setBatchJobsExpanded(!batchJobsExpanded)}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                {batchJobsExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <Clock className="h-3.5 w-3.5 text-amber-600" />
                <span className="text-xs font-semibold flex-1">BATCH JOBS</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                  {enabledBatchJobIndices.size > 0 ? `${enabledBatchJobIndices.size}/` : ''}{scheduledJobs.length}
                </Badge>
              </button>
              {batchJobsExpanded && (
                <div className="pl-4 space-y-0.5">
                  <div className="px-2 py-1 text-[10px] text-muted-foreground border-b border-dashed mb-1">
                    Click + to add to map
                  </div>
                  {scheduledJobs
                    .filter((j) => matchesSearch(j.name) || matchesSearch(j.command))
                    .slice(0, 20)
                    .map((job, i) => (
                      <ScheduledJobRow key={i} job={job} index={i} />
                    ))}
                  {scheduledJobs.length > 20 && (
                    <div className="px-2 py-1 text-[10px] text-muted-foreground">
                      +{scheduledJobs.length - 20} more jobs
                    </div>
                  )}
                </div>
              )}
            </>
          )}

          {/* EXTERNAL Section */}
          {externalTargets.length > 0 && (
            <>
              <button
                onClick={() => setExternalsExpanded(!externalsExpanded)}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                {externalsExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                <Cloud className="h-3.5 w-3.5 text-slate-400" />
                <span className="text-xs font-semibold flex-1">EXTERNAL</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                  {enabledExternalIndices.size > 0 ? `${enabledExternalIndices.size}/` : ''}{externalTargets.length}
                </Badge>
              </button>
              {externalsExpanded && (
                <div className="pl-4 space-y-0.5">
                  <div className="px-2 py-1 text-[10px] text-muted-foreground border-b border-dashed mb-1">
                    Click + to add to map
                  </div>
                  {externalTargets
                    .filter((c) => matchesSearch(c.remote_addr))
                    .map((conn, i) => (
                      <ExternalConnectionRow
                        key={i}
                        addr={conn.remote_addr}
                        port={conn.remote_port}
                        index={i}
                      />
                    ))}
                </div>
              )}
            </>
          )}
        </div>
      </ScrollArea>

      {/* Footer stats */}
      <div className="px-3 py-2 border-t border-border text-[10px] text-muted-foreground flex items-center justify-between">
        <span>{correlationResult?.agents_analyzed || 0} agents analyzed</span>
        <span>{enabledServiceIndices.size} selected</span>
      </div>
    </div>
  );
}
