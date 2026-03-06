import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useCreateApp, useCreateComponent, useAddDependency } from '@/api/apps';
import { useGateways, Gateway } from '@/api/gateways';
import { useAgents, Agent } from '@/api/agents';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem, SelectGroup, SelectLabel } from '@/components/ui/select';
import { ArrowLeft, ArrowRight, Check, Plus, Trash2, Server, Radio, AlertCircle } from 'lucide-react';

const STEPS = ['Welcome', 'App Info', 'Gateway', 'Components', 'Dependencies', 'Review', 'Done'] as const;

interface NewComponent {
  name: string;
  agent_id: string;
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

  // Fetch gateways and agents
  const { data: gateways = [], isLoading: gatewaysLoading } = useGateways();
  const { data: agents = [], isLoading: agentsLoading } = useAgents();

  const [step, setStep] = useState(0);
  const [appName, setAppName] = useState('');
  const [appDescription, setAppDescription] = useState('');
  const [selectedGatewayIds, setSelectedGatewayIds] = useState<string[]>([]);
  const [components, setComponents] = useState<NewComponent[]>([]);
  const [dependencies, setDependencies] = useState<NewDependency[]>([]);
  const [creating, setCreating] = useState(false);
  const [createdAppId, setCreatedAppId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Filter agents by selected gateways
  const availableAgents = useMemo(() => {
    if (selectedGatewayIds.length === 0) return agents;
    return agents.filter((a) => a.gateway_id && selectedGatewayIds.includes(a.gateway_id));
  }, [agents, selectedGatewayIds]);

  // Group agents by gateway for the selector
  const agentsByGateway = useMemo(() => {
    const map = new Map<string, { gateway: Gateway | null; agents: Agent[] }>();

    // Create a map of gateway_id to gateway
    const gatewayMap = new Map(gateways.map((g) => [g.id, g]));

    for (const agent of availableAgents) {
      const gwId = agent.gateway_id || 'none';
      if (!map.has(gwId)) {
        map.set(gwId, {
          gateway: agent.gateway_id ? gatewayMap.get(agent.gateway_id) || null : null,
          agents: [],
        });
      }
      map.get(gwId)!.agents.push(agent);
    }

    return Array.from(map.entries()).map(([gwId, data]) => ({
      gatewayId: gwId,
      gatewayName: data.gateway?.name || 'No Gateway',
      gatewayZone: data.gateway?.zone || '',
      agents: data.agents.sort((a, b) => a.hostname.localeCompare(b.hostname)),
    }));
  }, [availableAgents, gateways]);

  // Helper to get agent info by id
  const getAgent = (agentId: string): Agent | undefined => {
    return agents.find((a) => a.id === agentId);
  };

  const toggleGateway = (gwId: string) => {
    setSelectedGatewayIds((prev) =>
      prev.includes(gwId) ? prev.filter((id) => id !== gwId) : [...prev, gwId]
    );
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
    // Update dependencies to account for removed index
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
      const app = await createApp.mutateAsync({ name: appName, description: appDescription });
      const componentIds: string[] = [];

      for (const comp of components) {
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
      }

      for (const dep of dependencies) {
        if (componentIds[dep.from] && componentIds[dep.to]) {
          await addDependency.mutateAsync({
            app_id: app.id,
            from_component_id: componentIds[dep.from],
            to_component_id: componentIds[dep.to],
          });
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

  // Check if all components have agents assigned
  const allComponentsResolved = components.length > 0 && components.every((c) => c.name.trim() && c.agent_id);

  // Connected gateways only
  const connectedGateways = gateways.filter((g) => g.connected);

  return (
    <div className="max-w-2xl mx-auto space-y-6">
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

      {step === 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Welcome to AppControl</CardTitle>
            <CardDescription>Let's set up your first application. This wizard will guide you through creating an application, selecting gateways/agents, adding components, and defining dependencies.</CardDescription>
          </CardHeader>
          <CardFooter>
            <Button onClick={() => setStep(1)}>Get Started <ArrowRight className="h-4 w-4 ml-2" /></Button>
          </CardFooter>
        </Card>
      )}

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

      {step === 2 && (
        <Card>
          <CardHeader>
            <CardTitle>Select Gateways</CardTitle>
            <CardDescription>Choose which gateways host your application's agents. You can select multiple gateways.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {gatewaysLoading ? (
              <p className="text-sm text-muted-foreground">Loading gateways...</p>
            ) : connectedGateways.length === 0 ? (
              <div className="p-4 border border-dashed border-border rounded-md text-center">
                <AlertCircle className="h-8 w-8 mx-auto text-muted-foreground mb-2" />
                <p className="text-sm text-muted-foreground">No connected gateways found.</p>
                <p className="text-xs text-muted-foreground mt-1">Please ensure at least one gateway is online.</p>
              </div>
            ) : (
              <div className="space-y-2">
                {connectedGateways.map((gw) => {
                  const isSelected = selectedGatewayIds.includes(gw.id);
                  const gwAgentCount = agents.filter((a) => a.gateway_id === gw.id).length;
                  return (
                    <div
                      key={gw.id}
                      onClick={() => toggleGateway(gw.id)}
                      className={`p-3 border rounded-md cursor-pointer transition-colors ${
                        isSelected
                          ? 'border-primary bg-primary/5'
                          : 'border-border hover:border-muted-foreground/50'
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <div className={`h-8 w-8 rounded-full flex items-center justify-center ${
                          isSelected ? 'bg-primary text-primary-foreground' : 'bg-muted'
                        }`}>
                          <Radio className="h-4 w-4" />
                        </div>
                        <div className="flex-1">
                          <p className="font-medium">{gw.name}</p>
                          <p className="text-xs text-muted-foreground">Zone: {gw.zone} · {gwAgentCount} agent{gwAgentCount !== 1 ? 's' : ''}</p>
                        </div>
                        {isSelected && <Check className="h-5 w-5 text-primary" />}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
          <CardFooter className="justify-between">
            <Button variant="outline" onClick={() => setStep(1)}><ArrowLeft className="h-4 w-4 mr-2" /> Back</Button>
            <Button onClick={() => setStep(3)} disabled={selectedGatewayIds.length === 0}>
              Next <ArrowRight className="h-4 w-4 ml-2" />
            </Button>
          </CardFooter>
        </Card>
      )}

      {step === 3 && (
        <Card>
          <CardHeader>
            <CardTitle>Components</CardTitle>
            <CardDescription>Add the components of your application and assign each to an agent</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {agentsLoading ? (
              <p className="text-sm text-muted-foreground">Loading agents...</p>
            ) : availableAgents.length === 0 ? (
              <div className="p-4 border border-dashed border-border rounded-md text-center">
                <AlertCircle className="h-8 w-8 mx-auto text-muted-foreground mb-2" />
                <p className="text-sm text-muted-foreground">No agents available on selected gateways.</p>
                <Button variant="link" onClick={() => setStep(2)}>Select different gateways</Button>
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
                            <Select value={comp.agent_id} onValueChange={(v) => updateComponent(i, 'agent_id', v)}>
                              <SelectTrigger>
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
                                      {group.gatewayName} {group.gatewayZone && `(${group.gatewayZone})`}
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
                      <div className="space-y-2 pt-2 border-t border-border/50">
                        <p className="text-xs text-muted-foreground font-medium">Commands (shell)</p>
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
            <Button onClick={() => setStep(4)} disabled={!allComponentsResolved}>
              Next <ArrowRight className="h-4 w-4 ml-2" />
            </Button>
          </CardFooter>
        </Card>
      )}

      {step === 4 && (
        <Card>
          <CardHeader>
            <CardTitle>Dependencies</CardTitle>
            <CardDescription>Define startup dependencies between components</CardDescription>
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

      {step === 5 && (
        <Card>
          <CardHeader>
            <CardTitle>Review</CardTitle>
            <CardDescription>Review your application before creating</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <p className="text-sm text-muted-foreground">Application</p>
              <p className="font-medium">{appName}</p>
              {appDescription && <p className="text-sm text-muted-foreground">{appDescription}</p>}
            </div>
            <div>
              <p className="text-sm text-muted-foreground mb-2">Gateways ({selectedGatewayIds.length})</p>
              <div className="flex flex-wrap gap-2">
                {selectedGatewayIds.map((gwId) => {
                  const gw = gateways.find((g) => g.id === gwId);
                  return gw ? (
                    <Badge key={gwId} variant="outline">
                      <Radio className="h-3 w-3 mr-1" /> {gw.name}
                    </Badge>
                  ) : null;
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
                        <span className="text-muted-foreground">
                          <Server className="h-3 w-3 inline mr-1" />
                          {agent?.hostname || 'Unknown'}
                        </span>
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

      {step === 6 && (
        <Card>
          <CardHeader className="text-center">
            <div className="flex justify-center mb-4">
              <div className="h-16 w-16 rounded-full bg-green-100 flex items-center justify-center">
                <Check className="h-8 w-8 text-green-600" />
              </div>
            </div>
            <CardTitle>Application Created!</CardTitle>
            <CardDescription>Your application is ready. You can now view it on the map.</CardDescription>
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
