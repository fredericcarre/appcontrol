import { useMemo, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Input } from '@/components/ui/input';
import {
  Server, Cog, Cloud, Clock, ChevronDown, ChevronRight, Search,
  PanelLeftClose, PanelLeft, Box, ArrowRight,
} from 'lucide-react';
import { COMPONENT_TYPE_ICONS, type ComponentType } from '@/lib/colors';
import { useDiscoveryStore } from '@/stores/discovery';
import { useApps } from '@/api/apps';
import { cn } from '@/lib/utils';

export function LayerSidebar() {
  const {
    correlationResult,
    enabledServiceIndices,
    getEffectiveName,
    getEffectiveType,
    setSelectedServiceIndex,
    setHighlightedServiceIndex,
    showUnresolved,
    toggleShowUnresolved,
    showBatchJobs,
    toggleShowBatchJobs,
    searchQuery,
    setSearchQuery,
  } = useDiscoveryStore();

  const [collapsed, setCollapsed] = useState(false);
  const [hostsOpen, setHostsOpen] = useState(true);
  const [servicesOpen, setServicesOpen] = useState(true);
  const [appsOpen, setAppsOpen] = useState(true);

  const { data: existingApps } = useApps();
  const services = useMemo(() => correlationResult?.services || [], [correlationResult]);
  const unresolved = useMemo(() => correlationResult?.unresolved_connections || [], [correlationResult]);
  const scheduledJobs = useMemo(() => correlationResult?.scheduled_jobs || [], [correlationResult]);

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
                  {existingApps
                    .filter((app) => matchesSearch(app.name))
                    .slice(0, 10)
                    .map((app) => (
                      <a
                        key={app.id}
                        href={`/apps/${app.id}`}
                        className="flex items-center gap-2 w-full px-2 py-1 rounded hover:bg-accent text-left group"
                      >
                        <Box className="h-3 w-3 text-blue-400 flex-shrink-0" />
                        <span className="text-xs truncate flex-1">{app.name}</span>
                        <Badge variant="outline" className="text-[9px] px-1 py-0 h-4">
                          {app.component_count}
                        </Badge>
                        <ArrowRight className="h-3 w-3 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
                      </a>
                    ))}
                  {existingApps.length > 10 && (
                    <div className="px-2 py-1 text-[10px] text-muted-foreground">
                      +{existingApps.length - 10} more apps
                    </div>
                  )}
                </div>
              )}
            </>
          )}

          {/* BATCH JOBS Section */}
          {scheduledJobs.length > 0 && (
            <>
              <button
                onClick={toggleShowBatchJobs}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                <div className={cn('w-3 h-3 rounded border flex items-center justify-center', showBatchJobs ? 'bg-primary border-primary' : 'border-input')}>
                  {showBatchJobs && <div className="w-1.5 h-1.5 rounded-sm bg-primary-foreground" />}
                </div>
                <Clock className="h-3.5 w-3.5 text-amber-600" />
                <span className="text-xs font-semibold flex-1">BATCH JOBS</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">{scheduledJobs.length}</Badge>
              </button>
              {showBatchJobs && (
                <div className="pl-4 space-y-0.5">
                  {scheduledJobs
                    .filter((j) => matchesSearch(j.name) || matchesSearch(j.command))
                    .map((job, i) => (
                      <div key={i} className="flex items-center gap-2 px-2 py-1 text-left">
                        <Clock className="h-3 w-3 text-amber-500 flex-shrink-0" />
                        <span className="text-xs truncate flex-1" title={job.command}>{job.name}</span>
                        <span className="text-[9px] text-muted-foreground">{job.source}</span>
                      </div>
                    ))}
                </div>
              )}
            </>
          )}

          {/* EXTERNAL Section */}
          {externalTargets.length > 0 && (
            <>
              <button
                onClick={toggleShowUnresolved}
                className="flex items-center gap-1.5 w-full px-2 py-1.5 rounded-md hover:bg-accent text-left"
              >
                <div className={cn('w-3 h-3 rounded border flex items-center justify-center', showUnresolved ? 'bg-primary border-primary' : 'border-input')}>
                  {showUnresolved && <div className="w-1.5 h-1.5 rounded-sm bg-primary-foreground" />}
                </div>
                <Cloud className="h-3.5 w-3.5 text-slate-400" />
                <span className="text-xs font-semibold flex-1">EXTERNAL</span>
                <Badge variant="secondary" className="text-[10px] px-1.5 py-0">{externalTargets.length}</Badge>
              </button>
              {showUnresolved && (
                <div className="pl-4 space-y-0.5">
                  {externalTargets
                    .filter((c) => matchesSearch(c.remote_addr))
                    .map((conn, i) => (
                      <div key={i} className="flex items-center gap-2 px-2 py-1">
                        <Cloud className="h-3 w-3 text-slate-400 flex-shrink-0" />
                        <span className="text-xs truncate flex-1">{conn.remote_addr}</span>
                        <span className="text-[9px] font-mono text-muted-foreground">:{conn.remote_port}</span>
                      </div>
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
