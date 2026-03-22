import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useCreateApp, useCreateComponent, useAddDependency } from '@/api/apps';
import { useGatewaySites, SiteSummary } from '@/api/gateways';
import { useAgents, Agent } from '@/api/agents';
import { useCreateProfile, MappingConfig } from '@/api/import';
import client from '@/api/client';
import { SiteOverrideInput } from '@/api/sites';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem, SelectGroup, SelectLabel } from '@/components/ui/select';
import { ArrowLeft, ArrowRight, Check, Plus, Trash2, Server, MapPin, AlertCircle, Shield, ChevronDown, ChevronRight, Settings } from 'lucide-react';
import { SiteSelector, getGatewayIdsForSite, getSiteById } from '@/components/SiteSelector';

const STEPS = ['Welcome', 'App Info', 'Sites', 'Components', 'Dependencies', 'Review', 'Done'] as const;

interface SelectedSite {
  siteId: string;
  siteType: 'primary' | 'dr';
}

interface CommandOverrides {
  check_cmd?: string;
  start_cmd?: string;
  stop_cmd?: string;
}

interface ComponentSiteConfig {
  enabled: boolean;  // Whether component is available on this site
  agentId: string;
  commandOverrides?: CommandOverrides;
}

interface NewComponent {
  name: string;
  agent_id: string;  // Primary site agent
  component_type: string;
  check_cmd: string;
  start_cmd: string;
  stop_cmd: string;
}

interface NewDependency {
  from: number;
  to: number;
}

export function OnboardingPage() {
  const navigate = useNavigate();
  const createApp = useCreateApp();
  const createComponent = useCreateComponent();
  const addDependency = useAddDependency();
  const createProfile = useCreateProfile();

  // Fetch sites (with gateways) and agents
  const { data: sites = [], isLoading: sitesLoading } = useGatewaySites();
  const { data: agents = [], isLoading: agentsLoading } = useAgents();

  const [step, setStep] = useState(0);
  const [appName, setAppName] = useState('');
  const [appDescription, setAppDescription] = useState('');
  // Multi-site selection
  const [selectedSites, setSelectedSites] = useState<SelectedSite[]>([]);
  // Per-component, per-site configuration: componentIndex -> siteId -> config
  const [componentSiteConfigs, setComponentSiteConfigs] = useState<Record<number, Record<string, ComponentSiteConfig>>>({});
  // Track which components have expanded command overrides per site
  const [expandedOverrides, setExpandedOverrides] = useState<Record<string, boolean>>({});
  const [components, setComponents] = useState<NewComponent[]>([]);
  const [dependencies, setDependencies] = useState<NewDependency[]>([]);
  const [creating, setCreating] = useState(false);
  const [createdAppId, setCreatedAppId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Get primary site from selected sites
  const primarySiteId = useMemo(() => {
    const primary = selectedSites.find((s) => s.siteType === 'primary');
    return primary?.siteId || null;
  }, [selectedSites]);

  // Get DR sites from selected sites
  const drSites = useMemo(() => {
    return selectedSites.filter((s) => s.siteType === 'dr');
  }, [selectedSites]);

  // Get gateway IDs from selected site
  const selectedGatewayIds = useMemo(() => {
    return getGatewayIdsForSite(sites, primarySiteId);
  }, [sites, primarySiteId]);

  // Get gateway IDs for a specific site
  const getGatewayIdsForSiteId = (siteId: string) => {
    return getGatewayIdsForSite(sites, siteId);
  };

  // Get agents for a specific site
  const getAgentsForSite = (siteId: string) => {
    const gwIds = getGatewayIdsForSiteId(siteId);
    return agents.filter((a) => a.gateway_id && gwIds.includes(a.gateway_id));
  };

  // Filter agents by selected site's gateways (primary site)
  const availableAgents = useMemo(() => {
    if (selectedGatewayIds.length === 0) return [];
    return agents.filter((a) => a.gateway_id && selectedGatewayIds.includes(a.gateway_id));
  }, [agents, selectedGatewayIds]);

  // Group agents by gateway for a specific site
  const getAgentsByGatewayForSite = (siteId: string) => {
    const siteGwIds = getGatewayIdsForSiteId(siteId);
    const siteAgents = getAgentsForSite(siteId);
    const gatewayMap = new Map<string, { gatewayName: string; siteCode: string; agents: Agent[] }>();

    for (const site of sites) {
      for (const gw of site.gateways) {
        if (siteGwIds.includes(gw.id)) {
          gatewayMap.set(gw.id, {
            gatewayName: gw.name,
            siteCode: site.site_code,
            agents: [],
          });
        }
      }
    }

    for (const agent of siteAgents) {
      if (agent.gateway_id && gatewayMap.has(agent.gateway_id)) {
        gatewayMap.get(agent.gateway_id)!.agents.push(agent);
      }
    }

    return Array.from(gatewayMap.entries())
      .map(([gwId, data]) => ({
        gatewayId: gwId,
        ...data,
        agents: data.agents.sort((a, b) => a.hostname.localeCompare(b.hostname)),
      }))
      .filter((g) => g.agents.length > 0);
  };

  // Group agents by gateway for the primary site selector
  const agentsByGateway = useMemo(() => {
    if (!primarySiteId) return [];
    return getAgentsByGatewayForSite(primarySiteId);
  }, [sites, agents, primarySiteId]);

  // Helper to get agent info by id
  const getAgent = (agentId: string): Agent | undefined => {
    return agents.find((a) => a.id === agentId);
  };

  const addComponent = () => {
    setComponents([...components, {
      name: '',
      agent_id: '',
      component_type: 'service',
      check_cmd: '',
      start_cmd: '',
      stop_cmd: '',
    }]);
  };

  const removeComponent = (i: number) => {
    setComponents(components.filter((_, idx) => idx !== i));
    setDependencies(
      dependencies
        .filter((d) => d.from !== i && d.to !== i)
        .map((d) => ({
          from: d.from > i ? d.from - 1 : d.from,
          to: d.to > i ? d.to - 1 : d.to,
        }))
    );
  };

  const updateComponent = (i: number, field: keyof NewComponent, value: string) => {
    const next = [...components];
    next[i] = { ...next[i], [field]: value };
    setComponents(next);
  };

  const addDep = () => {
    if (components.length >= 2) {
      setDependencies([...dependencies, { from: 0, to: 1 }]);
    }
  };

  const handleCreate = async () => {
    setCreating(true);
    setError(null);
    try {
      // 1. Create the application
      const app = await createApp.mutateAsync({
        name: appName,
        description: appDescription,
        site_id: primarySiteId || undefined,
      });
      const componentIds: string[] = [];

      // 2. Create components and track name -> id mapping
      const componentNameToId: Record<string, string> = {};
      for (let compIndex = 0; compIndex < components.length; compIndex++) {
        const comp = components[compIndex];
        const agent = getAgent(comp.agent_id);
        const created = await createComponent.mutateAsync({
          app_id: app.id,
          name: comp.name,
          host: agent?.hostname || '',
          agent_id: comp.agent_id || undefined,
          component_type: comp.component_type,
          check_cmd: comp.check_cmd || undefined,
          start_cmd: comp.start_cmd || undefined,
          stop_cmd: comp.stop_cmd || undefined,
        });
        componentIds.push(created.id);
        componentNameToId[comp.name] = created.id;
      }

      // 3. Create dependencies
      for (const dep of dependencies) {
        if (componentIds[dep.from] && componentIds[dep.to]) {
          await addDependency.mutateAsync({
            app_id: app.id,
            from_component_id: componentIds[dep.from],
            to_component_id: componentIds[dep.to],
          });
        }
      }

      // 4. Create binding profiles for primary site
      if (primarySiteId) {
        const primarySiteInfo = getSiteById(sites, primarySiteId);
        const primaryMappings: MappingConfig[] = components.map((comp) => ({
          component_name: comp.name,
          agent_id: comp.agent_id,
          resolved_via: 'wizard',
        }));

        await createProfile.mutateAsync({
          appId: app.id,
          name: primarySiteInfo?.site_code?.toLowerCase() || 'primary',
          description: `Primary configuration for ${primarySiteInfo?.site_name || 'default site'}`,
          profile_type: 'primary',
          gateway_ids: getGatewayIdsForSiteId(primarySiteId),
          mappings: primaryMappings,
        });
      }

      // 5. Create binding profiles for each DR site
      for (const drSite of drSites) {
        const drSiteInfo = getSiteById(sites, drSite.siteId);
        const drMappings: MappingConfig[] = components
          .map((comp, compIndex) => {
            const siteConfig = componentSiteConfigs[compIndex]?.[drSite.siteId];
            // Skip disabled components
            if (siteConfig?.enabled === false) return null;
            return {
              component_name: comp.name,
              agent_id: siteConfig?.agentId || '',
              resolved_via: 'wizard',
            };
          })
          .filter((m): m is MappingConfig => m !== null && !!m.agent_id); // Only include enabled mappings with agents

        if (drMappings.length > 0) {
          await createProfile.mutateAsync({
            appId: app.id,
            name: drSiteInfo?.site_code?.toLowerCase() || `dr-${drSite.siteId.slice(0, 8)}`,
            description: `DR configuration for ${drSiteInfo?.site_name || 'DR site'}`,
            profile_type: 'dr',
            gateway_ids: getGatewayIdsForSiteId(drSite.siteId),
            auto_failover: false,
            mappings: drMappings,
          });
        }
      }

      // 6. Create site overrides for command overrides
      for (const drSite of drSites) {
        const drSiteInfo = getSiteById(sites, drSite.siteId);
        if (!drSiteInfo) continue;

        for (let compIndex = 0; compIndex < components.length; compIndex++) {
          const comp = components[compIndex];
          const siteConfig = componentSiteConfigs[compIndex]?.[drSite.siteId];

          // Skip disabled components
          if (siteConfig?.enabled === false) continue;

          const overrides = siteConfig?.commandOverrides;

          // Only create site override if there are command overrides
          if (overrides && (overrides.check_cmd || overrides.start_cmd || overrides.stop_cmd)) {
            const componentId = componentNameToId[comp.name];
            if (componentId) {
              const payload: SiteOverrideInput = {
                site_id: drSite.siteId,
                check_cmd_override: overrides.check_cmd || null,
                start_cmd_override: overrides.start_cmd || null,
                stop_cmd_override: overrides.stop_cmd || null,
              };
              await client.put(`/components/${componentId}/site-overrides/${drSite.siteId}`, payload);
            }
          }
        }
      }

      setCreatedAppId(app.id);
      setStep(6);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create application';
      setError(message);
      console.error('Create app error:', err);
    } finally {
      setCreating(false);
    }
  };

  // Check if all components have agents assigned for primary site
  const allComponentsResolved = components.length > 0 && components.every((c) => c.name.trim() && c.agent_id);

  // Check if all DR sites have agents assigned for each enabled component
  const allDrSitesResolved = drSites.every((drSite) => {
    return components.every((_, compIndex) => {
      const siteConfig = componentSiteConfigs[compIndex]?.[drSite.siteId];
      // If not enabled (explicitly disabled), no agent needed
      if (siteConfig?.enabled === false) return true;
      // If enabled (default), must have an agent
      return siteConfig?.agentId;
    });
  });

  // Get selected site info
  const primarySite = getSiteById(sites, primarySiteId);

  // Sites available for DR (exclude primary and already selected DR sites)
  const drAvailableSites = sites.filter((s) =>
    s.site_id !== primarySiteId && !drSites.some((dr) => dr.siteId === s.site_id)
  );

  // Helper to add a DR site
  const addDrSite = (siteId: string) => {
    setSelectedSites([...selectedSites, { siteId, siteType: 'dr' }]);
  };

  // Helper to remove a DR site
  const removeDrSite = (siteId: string) => {
    setSelectedSites(selectedSites.filter((s) => !(s.siteType === 'dr' && s.siteId === siteId)));
    // Clean up component configs for this site
    const newConfigs = { ...componentSiteConfigs };
    for (const compIndex of Object.keys(newConfigs)) {
      if (newConfigs[Number(compIndex)][siteId]) {
        delete newConfigs[Number(compIndex)][siteId];
      }
    }
    setComponentSiteConfigs(newConfigs);
  };

  // Helper to update component site config
  const updateComponentSiteConfig = (
    compIndex: number,
    siteId: string,
    field: keyof ComponentSiteConfig | 'check_cmd' | 'start_cmd' | 'stop_cmd',
    value: string | boolean
  ) => {
    setComponentSiteConfigs((prev) => {
      const compConfigs = prev[compIndex] || {};
      const siteConfig = compConfigs[siteId] || { enabled: true, agentId: '' };

      if (field === 'enabled') {
        return {
          ...prev,
          [compIndex]: {
            ...compConfigs,
            [siteId]: { ...siteConfig, enabled: value as boolean },
          },
        };
      } else if (field === 'agentId') {
        return {
          ...prev,
          [compIndex]: {
            ...compConfigs,
            [siteId]: { ...siteConfig, agentId: value as string },
          },
        };
      } else {
        // Command override field
        const overrides = siteConfig.commandOverrides || {};
        return {
          ...prev,
          [compIndex]: {
            ...compConfigs,
            [siteId]: {
              ...siteConfig,
              commandOverrides: { ...overrides, [field]: (value as string) || undefined },
            },
          },
        };
      }
    });
  };

  // Toggle override expansion for a component+site
  const toggleOverrideExpansion = (compIndex: number, siteId: string) => {
    const key = `${compIndex}-${siteId}`;
    setExpandedOverrides((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6">
      {/* Progress indicator */}
      <div className="flex gap-2 items-center flex-wrap">
        {STEPS.map((s, i) => (
          <div key={s} className="flex items-center gap-2">
            <div className={`h-8 w-8 rounded-full flex items-center justify-center text-xs font-medium ${
              i < step ? 'bg-primary text-primary-foreground' :
              i === step ? 'bg-primary text-primary-foreground' :
              'bg-muted text-muted-foreground'
            }`}>
              {i < step ? <Check className="h-4 w-4" /> : i + 1}
            </div>
            {i < STEPS.length - 1 && <div className="w-6 h-0.5 bg-border" />}
          </div>
        ))}
      </div>

      {/* Step 0: Welcome */}
      {step === 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Welcome to AppControl</CardTitle>
            <CardDescription>
              Let's set up your first application. This wizard will guide you through:
              selecting a site, adding components, and defining dependencies.
            </CardDescription>
          </CardHeader>
          <CardFooter>
            <Button onClick={() => setStep(1)}>Get Started <ArrowRight className="h-4 w-4 ml-2" /></Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 1: App Info */}
      {step === 1 && (
        <Card>
          <CardHeader>
            <CardTitle>Application Info</CardTitle>
            <CardDescription>Name and describe your application</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">Name</label>
              <Input value={appName} onChange={(e) => setAppName(e.target.value)} placeholder="My Application" />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Description</label>
              <Input value={appDescription} onChange={(e) => setAppDescription(e.target.value)} placeholder="What does this application do?" />
            </div>
          </CardContent>
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(0)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={() => setStep(2)} disabled={!appName.trim()}>Next <ArrowRight className="h-4 w-4 ml-2" /></Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 2: Site Selection (Multi-site) */}
      {step === 2 && (
        <Card>
          <CardHeader>
            <CardTitle>Select Sites</CardTitle>
            <CardDescription>Choose where your application will run. You can configure multiple DR sites.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-6">
            {sitesLoading ? (
              <p className="text-sm text-muted-foreground">Loading sites...</p>
            ) : (
              <>
                {/* Primary Site */}
                <SiteSelector
                  sites={sites}
                  selectedSiteId={primarySiteId}
                  onSelect={(siteId) => {
                    // Remove old primary and add new one
                    const nonPrimary = selectedSites.filter((s) => s.siteType !== 'primary');
                    if (siteId) {
                      setSelectedSites([{ siteId, siteType: 'primary' }, ...nonPrimary]);
                    } else {
                      setSelectedSites(nonPrimary);
                    }
                  }}
                  label="Primary Site"
                  description="Select the site where your application will run."
                  emptyMessage="No sites with connected gateways. Please ensure at least one gateway is online."
                  variant="primary"
                />

                {/* DR Sites Section */}
                <div className="border-t pt-4 space-y-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <span className="font-medium">DR Sites</span>
                      <p className="text-muted-foreground text-sm">
                        Configure failover sites for disaster recovery. You can add multiple DR sites.
                      </p>
                    </div>
                  </div>

                  {/* Selected DR Sites */}
                  {drSites.map((drSite) => {
                    const site = getSiteById(sites, drSite.siteId);
                    if (!site) return null;
                    return (
                      <div
                        key={drSite.siteId}
                        className="p-3 border rounded-md border-orange-200 dark:border-orange-800 bg-orange-50/50 dark:bg-orange-950/30"
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <Shield className="h-4 w-4 text-orange-600" />
                            <div>
                              <p className="font-medium">{site.site_name}</p>
                              <p className="text-xs text-muted-foreground font-mono">{site.site_code}</p>
                            </div>
                            <Badge variant="outline" className="ml-2 text-xs">
                              {site.gateways.length} gateway{site.gateways.length !== 1 ? 's' : ''}
                            </Badge>
                          </div>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => removeDrSite(drSite.siteId)}
                          >
                            <Trash2 className="h-4 w-4 text-destructive" />
                          </Button>
                        </div>
                      </div>
                    );
                  })}

                  {/* Add DR Site button */}
                  {drAvailableSites.length > 0 && (
                    <div className="space-y-2">
                      <Select
                        value=""
                        onValueChange={(siteId) => {
                          if (siteId) addDrSite(siteId);
                        }}
                      >
                        <SelectTrigger className="w-full">
                          <SelectValue placeholder="Add DR site..." />
                        </SelectTrigger>
                        <SelectContent>
                          {drAvailableSites.filter((site) => site.site_id).map((site) => (
                            <SelectItem key={site.site_id!} value={site.site_id!}>
                              <span className="flex items-center gap-2">
                                <Shield className="h-3 w-3 text-orange-600" />
                                {site.site_name}
                                <span className="text-muted-foreground text-xs">({site.site_code})</span>
                              </span>
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>
                  )}

                  {drAvailableSites.length === 0 && drSites.length === 0 && (
                    <p className="text-sm text-muted-foreground">No additional sites available for DR configuration.</p>
                  )}
                </div>
              </>
            )}
          </CardContent>
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(1)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={() => setStep(3)} disabled={!primarySiteId}>
              Next <ArrowRight className="h-4 w-4 ml-2" />
            </Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 3: Components */}
      {step === 3 && (
        <Card>
          <CardHeader>
            <CardTitle>Components</CardTitle>
            <CardDescription>
              Add the components of your application and assign each to an agent.
              {drSites.length > 0 && ' Configure agents for each site.'}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {agentsLoading ? (
              <p className="text-sm text-muted-foreground">Loading agents...</p>
            ) : availableAgents.length === 0 ? (
              <div className="p-4 border border-dashed border-border rounded-md text-center">
                <AlertCircle className="h-8 w-8 mx-auto text-muted-foreground mb-2" />
                <p className="text-sm text-muted-foreground">No agents available on the selected site.</p>
                <Button variant="link" onClick={() => setStep(2)}>Select a different site</Button>
              </div>
            ) : (
              <>
                {components.map((comp, i) => {
                  const selectedAgent = comp.agent_id ? getAgent(comp.agent_id) : null;
                  return (
                    <div key={i} className="p-3 border border-border rounded-md space-y-3">
                      <div className="flex gap-2 items-start">
                        <div className="flex-1 space-y-2">
                          <Input
                            placeholder="Component name"
                            value={comp.name}
                            onChange={(e) => updateComponent(i, 'name', e.target.value)}
                          />
                          <div className="grid grid-cols-2 gap-2">
                            <Select value={comp.component_type} onValueChange={(v) => updateComponent(i, 'component_type', v)}>
                              <SelectTrigger><SelectValue /></SelectTrigger>
                              <SelectContent>
                                <SelectItem value="database">Database</SelectItem>
                                <SelectItem value="middleware">Middleware</SelectItem>
                                <SelectItem value="appserver">App Server</SelectItem>
                                <SelectItem value="webfront">Web Frontend</SelectItem>
                                <SelectItem value="service">Service</SelectItem>
                                <SelectItem value="batch">Batch</SelectItem>
                              </SelectContent>
                            </Select>
                          </div>
                        </div>
                        <Button variant="ghost" size="icon" onClick={() => removeComponent(i)}>
                          <Trash2 className="h-4 w-4 text-destructive" />
                        </Button>
                      </div>

                      {/* Primary Site Commands */}
                      <div className="space-y-2 pt-2 border-t border-border/50">
                        <p className="text-xs text-muted-foreground font-medium">Commands (shell) - used by default on all sites</p>
                        <Input
                          placeholder="Check command (e.g., pgrep -f myprocess)"
                          value={comp.check_cmd}
                          onChange={(e) => updateComponent(i, 'check_cmd', e.target.value)}
                          className="font-mono text-sm"
                        />
                        <div className="grid grid-cols-2 gap-2">
                          <Input
                            placeholder="Start command"
                            value={comp.start_cmd}
                            onChange={(e) => updateComponent(i, 'start_cmd', e.target.value)}
                            className="font-mono text-sm"
                          />
                          <Input
                            placeholder="Stop command"
                            value={comp.stop_cmd}
                            onChange={(e) => updateComponent(i, 'stop_cmd', e.target.value)}
                            className="font-mono text-sm"
                          />
                        </div>
                      </div>

                      {/* Site Agent Assignments */}
                      <div className="space-y-2 pt-2 border-t border-border/50">
                        <p className="text-xs text-muted-foreground font-medium">Site Agent Assignments</p>

                        {/* Primary Site */}
                        <div className="p-2 bg-blue-50/50 dark:bg-blue-950/30 rounded border border-blue-200 dark:border-blue-800">
                          <div className="flex items-center gap-2 mb-2">
                            <MapPin className="h-3 w-3 text-blue-600" />
                            <span className="text-xs font-medium">{primarySite?.site_name || 'Primary'}</span>
                            <Badge variant="outline" className="text-xs">Primary</Badge>
                          </div>
                          <Select value={comp.agent_id} onValueChange={(v) => updateComponent(i, 'agent_id', v)}>
                            <SelectTrigger className="h-8">
                              <SelectValue placeholder="Select agent...">
                                {selectedAgent && (
                                  <span className="flex items-center gap-2">
                                    <Server className="h-3 w-3" />
                                    {selectedAgent.hostname}
                                  </span>
                                )}
                              </SelectValue>
                            </SelectTrigger>
                            <SelectContent>
                              {agentsByGateway.map((group) => (
                                <SelectGroup key={group.gatewayId}>
                                  <SelectLabel className="text-xs text-muted-foreground">
                                    {group.gatewayName}
                                  </SelectLabel>
                                  {group.agents.map((agent) => (
                                    <SelectItem key={agent.id} value={agent.id}>
                                      <span className="flex items-center gap-2">
                                        <span className={`h-2 w-2 rounded-full ${agent.connected ? 'bg-green-500' : 'bg-gray-400'}`} />
                                        {agent.hostname}
                                      </span>
                                    </SelectItem>
                                  ))}
                                </SelectGroup>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>

                        {/* DR Sites */}
                        {drSites.map((drSite) => {
                          const site = getSiteById(sites, drSite.siteId);
                          if (!site) return null;
                          const siteAgents = getAgentsByGatewayForSite(drSite.siteId);
                          const siteConfig = componentSiteConfigs[i]?.[drSite.siteId];
                          const isEnabled = siteConfig?.enabled !== false; // Default to true
                          const selectedDrAgent = siteConfig?.agentId ? getAgent(siteConfig.agentId) : null;
                          const overrideKey = `${i}-${drSite.siteId}`;
                          const isExpanded = expandedOverrides[overrideKey];
                          const hasOverrides = siteConfig?.commandOverrides &&
                            (siteConfig.commandOverrides.check_cmd ||
                             siteConfig.commandOverrides.start_cmd ||
                             siteConfig.commandOverrides.stop_cmd);

                          return (
                            <div
                              key={drSite.siteId}
                              className={`p-2 rounded border ${
                                isEnabled
                                  ? 'bg-orange-50/50 dark:bg-orange-950/30 border-orange-200 dark:border-orange-800'
                                  : 'bg-muted/30 border-border opacity-60'
                              }`}
                            >
                              <div className="flex items-center justify-between mb-2">
                                <div className="flex items-center gap-2">
                                  <Shield className={`h-3 w-3 ${isEnabled ? 'text-orange-600' : 'text-muted-foreground'}`} />
                                  <span className="text-xs font-medium">{site.site_name}</span>
                                  <Badge variant="outline" className={`text-xs ${isEnabled ? 'text-orange-600' : 'text-muted-foreground'}`}>DR</Badge>
                                  {isEnabled && hasOverrides && (
                                    <span title="Has command overrides">
                                      <Settings className="h-3 w-3 text-orange-600" />
                                    </span>
                                  )}
                                </div>
                                <label className="flex items-center gap-1.5 cursor-pointer">
                                  <span className="text-xs text-muted-foreground">
                                    {isEnabled ? 'Enabled' : 'Disabled'}
                                  </span>
                                  <input
                                    type="checkbox"
                                    checked={isEnabled}
                                    onChange={(e) => updateComponentSiteConfig(i, drSite.siteId, 'enabled', e.target.checked)}
                                    className="h-3.5 w-3.5 rounded"
                                  />
                                </label>
                              </div>

                              {isEnabled ? (
                                <>
                                  <Select
                                    value={siteConfig?.agentId || ''}
                                    onValueChange={(v) => updateComponentSiteConfig(i, drSite.siteId, 'agentId', v)}
                                  >
                                    <SelectTrigger className="h-8">
                                      <SelectValue placeholder="Select agent...">
                                        {selectedDrAgent && (
                                          <span className="flex items-center gap-2">
                                            <Server className="h-3 w-3" />
                                            {selectedDrAgent.hostname}
                                          </span>
                                        )}
                                      </SelectValue>
                                    </SelectTrigger>
                                    <SelectContent>
                                      {siteAgents.length === 0 ? (
                                        <p className="p-2 text-sm text-muted-foreground">No agents on this site</p>
                                      ) : (
                                        siteAgents.map((group) => (
                                          <SelectGroup key={group.gatewayId}>
                                            <SelectLabel className="text-xs text-muted-foreground">
                                              {group.gatewayName}
                                            </SelectLabel>
                                            {group.agents.map((agent) => (
                                              <SelectItem key={agent.id} value={agent.id}>
                                                <span className="flex items-center gap-2">
                                                  <span className={`h-2 w-2 rounded-full ${agent.connected ? 'bg-green-500' : 'bg-gray-400'}`} />
                                                  {agent.hostname}
                                                </span>
                                              </SelectItem>
                                            ))}
                                          </SelectGroup>
                                        ))
                                      )}
                                    </SelectContent>
                                  </Select>

                                  {/* Command Overrides Toggle */}
                                  <button
                                    type="button"
                                    className="flex items-center gap-1 mt-2 text-xs text-muted-foreground hover:text-foreground"
                                    onClick={() => toggleOverrideExpansion(i, drSite.siteId)}
                                  >
                                    {isExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                                    Command overrides (optional)
                                  </button>

                                  {/* Command Overrides Fields */}
                                  {isExpanded && (
                                    <div className="mt-2 space-y-2">
                                      <Input
                                        placeholder="Check command override"
                                        value={siteConfig?.commandOverrides?.check_cmd || ''}
                                        onChange={(e) => updateComponentSiteConfig(i, drSite.siteId, 'check_cmd', e.target.value)}
                                        className="font-mono text-xs h-7"
                                      />
                                      <div className="grid grid-cols-2 gap-2">
                                        <Input
                                          placeholder="Start command override"
                                          value={siteConfig?.commandOverrides?.start_cmd || ''}
                                          onChange={(e) => updateComponentSiteConfig(i, drSite.siteId, 'start_cmd', e.target.value)}
                                          className="font-mono text-xs h-7"
                                        />
                                        <Input
                                          placeholder="Stop command override"
                                          value={siteConfig?.commandOverrides?.stop_cmd || ''}
                                          onChange={(e) => updateComponentSiteConfig(i, drSite.siteId, 'stop_cmd', e.target.value)}
                                          className="font-mono text-xs h-7"
                                        />
                                      </div>
                                    </div>
                                  )}
                                </>
                              ) : (
                                <p className="text-xs text-muted-foreground italic">
                                  Component not replicated to this site
                                </p>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
                <Button variant="outline" onClick={addComponent} className="w-full">
                  <Plus className="h-4 w-4 mr-2" /> Add Component
                </Button>
              </>
            )}
          </CardContent>
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(2)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={() => setStep(4)} disabled={!allComponentsResolved || (drSites.length > 0 && !allDrSitesResolved)}>
              Next <ArrowRight className="h-4 w-4 ml-2" />
            </Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 4: Dependencies */}
      {step === 4 && (
        <Card>
          <CardHeader>
            <CardTitle>Dependencies</CardTitle>
            <CardDescription>Define startup dependencies between components (optional)</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {dependencies.map((dep, i) => (
              <div key={i} className="flex gap-2 items-center">
                <Select value={String(dep.from)} onValueChange={(v) => {
                  const next = [...dependencies]; next[i] = { ...next[i], from: parseInt(v) }; setDependencies(next);
                }}>
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {components.map((c, ci) => <SelectItem key={ci} value={String(ci)}>{c.name || `Component ${ci + 1}`}</SelectItem>)}
                  </SelectContent>
                </Select>
                <span className="text-muted-foreground text-sm">depends on</span>
                <Select value={String(dep.to)} onValueChange={(v) => {
                  const next = [...dependencies]; next[i] = { ...next[i], to: parseInt(v) }; setDependencies(next);
                }}>
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {components.map((c, ci) => <SelectItem key={ci} value={String(ci)}>{c.name || `Component ${ci + 1}`}</SelectItem>)}
                  </SelectContent>
                </Select>
                <Button variant="ghost" size="icon" onClick={() => setDependencies(dependencies.filter((_, idx) => idx !== i))}>
                  <Trash2 className="h-4 w-4 text-destructive" />
                </Button>
              </div>
            ))}
            <Button variant="outline" onClick={addDep} className="w-full" disabled={components.length < 2}>
              <Plus className="h-4 w-4 mr-2" /> Add Dependency
            </Button>
          </CardContent>
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(3)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={() => setStep(5)}>Next <ArrowRight className="h-4 w-4 ml-2" /></Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 5: Review */}
      {step === 5 && (
        <Card>
          <CardHeader>
            <CardTitle>Review</CardTitle>
            <CardDescription>Review your application before creating</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <p className="text-sm text-muted-foreground">Application</p>
              <p className="font-medium text-lg">{appName}</p>
              {appDescription && <p className="text-sm text-muted-foreground">{appDescription}</p>}
            </div>

            {/* Site info */}
            <div className="space-y-2">
              <div className="flex gap-3 flex-wrap">
                <div className="flex-1 min-w-[200px] p-3 border rounded-md border-blue-200 dark:border-blue-800 bg-blue-50/50 dark:bg-blue-950/30">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                    <MapPin className="h-4 w-4 text-blue-600" />
                    Primary Site
                  </div>
                  <p className="font-medium">{primarySite?.site_name}</p>
                  <p className="text-xs text-muted-foreground font-mono">{primarySite?.site_code}</p>
                </div>
                {drSites.map((drSiteEntry) => {
                  const site = getSiteById(sites, drSiteEntry.siteId);
                  if (!site) return null;
                  return (
                    <div
                      key={drSiteEntry.siteId}
                      className="flex-1 min-w-[200px] p-3 border rounded-md border-orange-200 dark:border-orange-800 bg-orange-50/50 dark:bg-orange-950/30"
                    >
                      <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                        <Shield className="h-4 w-4 text-orange-600" />
                        DR Site
                      </div>
                      <p className="font-medium">{site.site_name}</p>
                      <p className="text-xs text-muted-foreground font-mono">{site.site_code}</p>
                    </div>
                  );
                })}
              </div>
            </div>

            <div>
              <p className="text-sm text-muted-foreground mb-2">Components ({components.length})</p>
              <div className="space-y-2">
                {components.map((c, i) => {
                  const agent = getAgent(c.agent_id);
                  return (
                    <div key={i} className="p-2 bg-muted/50 rounded text-sm">
                      <div className="flex items-center gap-2">
                        <Badge variant="outline">{c.component_type}</Badge>
                        <span className="font-medium">{c.name}</span>
                      </div>

                      {/* Site assignments */}
                      <div className="mt-2 space-y-1">
                        <div className="flex items-center gap-2 text-xs">
                          <MapPin className="h-3 w-3 text-blue-600" />
                          <span className="text-muted-foreground">{primarySite?.site_code}:</span>
                          <Server className="h-3 w-3" />
                          <span>{agent?.hostname || 'Unknown'}</span>
                        </div>
                        {drSites.map((drSiteEntry) => {
                          const site = getSiteById(sites, drSiteEntry.siteId);
                          const siteConfig = componentSiteConfigs[i]?.[drSiteEntry.siteId];
                          const isEnabled = siteConfig?.enabled !== false;
                          const drAgent = siteConfig?.agentId ? getAgent(siteConfig.agentId) : null;
                          const hasOverrides = siteConfig?.commandOverrides &&
                            (siteConfig.commandOverrides.check_cmd ||
                             siteConfig.commandOverrides.start_cmd ||
                             siteConfig.commandOverrides.stop_cmd);
                          return (
                            <div key={drSiteEntry.siteId} className={`flex items-center gap-2 text-xs ${!isEnabled ? 'opacity-50' : ''}`}>
                              <Shield className={`h-3 w-3 ${isEnabled ? 'text-orange-600' : 'text-muted-foreground'}`} />
                              <span className="text-muted-foreground">{site?.site_code}:</span>
                              {isEnabled ? (
                                <>
                                  <Server className="h-3 w-3" />
                                  <span>{drAgent?.hostname || 'Unknown'}</span>
                                  {hasOverrides && (
                                    <span title="Has command overrides">
                                      <Settings className="h-3 w-3 text-orange-600" />
                                    </span>
                                  )}
                                </>
                              ) : (
                                <span className="italic text-muted-foreground">Not replicated</span>
                              )}
                            </div>
                          );
                        })}
                      </div>

                      {(c.check_cmd || c.start_cmd || c.stop_cmd) && (
                        <div className="mt-1 text-xs text-muted-foreground font-mono">
                          {c.check_cmd && <div>check: {c.check_cmd}</div>}
                          {c.start_cmd && <div>start: {c.start_cmd}</div>}
                          {c.stop_cmd && <div>stop: {c.stop_cmd}</div>}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            {dependencies.length > 0 && (
              <div>
                <p className="text-sm text-muted-foreground mb-2">Dependencies ({dependencies.length})</p>
                <div className="space-y-1 text-sm">
                  {dependencies.map((d, i) => (
                    <p key={i}>{components[d.from]?.name} → {components[d.to]?.name}</p>
                  ))}
                </div>
              </div>
            )}
          </CardContent>
          {error && (
            <CardContent className="pt-0">
              <div className="p-3 bg-red-50 border border-red-200 rounded-md text-sm text-red-700">
                {error}
              </div>
            </CardContent>
          )}
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(4)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={handleCreate} disabled={creating}>
              {creating ? 'Creating...' : 'Create Application'}
            </Button>
          </CardFooter>
        </Card>
      )}

      {/* Step 6: Done */}
      {step === 6 && (
        <Card>
          <CardHeader className="text-center">
            <div className="flex justify-center mb-4">
              <div className="h-16 w-16 rounded-full bg-green-100 flex items-center justify-center">
                <Check className="h-8 w-8 text-green-600" />
              </div>
            </div>
            <CardTitle>Application Created!</CardTitle>
            <CardDescription>
              Your application is ready. You can now view it on the map.
              {drSites.length > 0 && (
                <span className="block mt-2 text-orange-600">
                  {drSites.length} DR site{drSites.length > 1 ? 's' : ''} configured with binding profiles.
                </span>
              )}
            </CardDescription>
          </CardHeader>
          <CardFooter className="justify-center gap-3">
            <Button variant="outline" onClick={() => navigate('/')}>Go to Dashboard</Button>
            {createdAppId && (
              <Button onClick={() => navigate(`/apps/${createdAppId}`)}>View Map</Button>
            )}
          </CardFooter>
        </Card>
      )}
    </div>
  );
}
