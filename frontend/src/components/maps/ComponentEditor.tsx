import { useState, useMemo } from 'react';
import { Component, useComponentGroups, useApps } from '@/api/apps';
import { useAgents } from '@/api/reports';
import { useSites, useComponentSiteOverrides, useUpsertSiteOverride, useDeleteSiteOverride } from '@/api/sites';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useComponentTypes } from '@/hooks/use-component-types';
import { AlertCircle, Shield, Trash2, Plus, MapPin, Search, Folder, Server } from 'lucide-react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ICON_MAP } from '@/lib/icons';

const ICONS = ICON_MAP;

export interface ComponentFormData {
  name: string;
  display_name: string;
  description: string;
  component_type: string;
  icon: string;
  host: string;
  group_id: string | null;
  check_cmd: string;
  start_cmd: string;
  stop_cmd: string;
  // Timeouts and intervals
  check_interval_seconds: number;
  start_timeout_seconds: number;
  stop_timeout_seconds: number;
  is_optional: boolean;
  // For application-type components (referencing another app)
  referenced_app_id?: string | null;
  // Cluster configuration
  cluster_size?: number | null;
  cluster_nodes?: string[];
}

interface ComponentEditorProps {
  component: Component | null;
  appId: string;
  open: boolean;
  onClose: () => void;
  onSave: (data: ComponentFormData) => void;
  isCreating?: boolean;
  initialType?: string;
}

export function ComponentEditor({
  component,
  appId,
  open,
  onClose,
  onSave,
  isCreating = false,
  initialType,
}: ComponentEditorProps) {
  const { types: catalogTypes } = useComponentTypes();
  const { data: groups } = useComponentGroups(appId);
  const { data: agents } = useAgents();
  const { data: existingApps } = useApps();
  const { data: sites } = useSites();
  const { data: siteOverrides, refetch: refetchOverrides } = useComponentSiteOverrides(component?.id || '');
  const upsertOverride = useUpsertSiteOverride(component?.id || '');
  const deleteOverride = useDeleteSiteOverride(component?.id || '');

  // State for editing a site override
  const [editingOverride, setEditingOverride] = useState<{
    siteId: string;
    agentId: string | null;
    checkCmd: string;
    startCmd: string;
    stopCmd: string;
    rebuildCmd: string;
  } | null>(null);

  // Filter out the current app from the list (can't reference itself)
  const availableApps = useMemo(
    () => existingApps?.filter((app) => app.id !== appId) || [],
    [existingApps, appId]
  );

  // Compute initial form data based on component or initialType
  const initialFormData = useMemo((): ComponentFormData => {
    if (component) {
      return {
        name: component.name || '',
        display_name: component.display_name || '',
        description: component.description || '',
        component_type: component.component_type || 'service',
        icon: component.icon || 'cog',
        host: component.host || component.agent_hostname || '',
        group_id: component.group_id,
        check_cmd: component.check_cmd || '',
        start_cmd: component.start_cmd || '',
        stop_cmd: component.stop_cmd || '',
        check_interval_seconds: component.check_interval_seconds ?? 30,
        start_timeout_seconds: component.start_timeout_seconds ?? 120,
        stop_timeout_seconds: component.stop_timeout_seconds ?? 60,
        is_optional: component.is_optional ?? false,
        referenced_app_id: (component as Component & { referenced_app_id?: string }).referenced_app_id || null,
        cluster_size: component.cluster_size ?? null,
        cluster_nodes: component.cluster_nodes ?? [],
      };
    }
    if (initialType) {
      const typeInfo = catalogTypes.find((t) => t.type === initialType);
      return {
        name: '',
        display_name: '',
        description: '',
        component_type: initialType,
        icon: typeInfo?.iconName || 'box',
        host: '',
        group_id: null,
        check_cmd: typeInfo?.defaultCheckCmd || '',
        start_cmd: typeInfo?.defaultStartCmd || '',
        stop_cmd: typeInfo?.defaultStopCmd || '',
        check_interval_seconds: 30,
        start_timeout_seconds: 120,
        stop_timeout_seconds: 60,
        is_optional: false,
        referenced_app_id: null,
        cluster_size: null,
        cluster_nodes: [],
      };
    }
    return {
      name: '',
      display_name: '',
      description: '',
      component_type: 'service',
      icon: 'cog',
      host: '',
      group_id: null,
      check_cmd: '',
      start_cmd: '',
      stop_cmd: '',
      check_interval_seconds: 30,
      start_timeout_seconds: 120,
      stop_timeout_seconds: 60,
      is_optional: false,
      referenced_app_id: null,
      cluster_size: null,
      cluster_nodes: [],
    };
  }, [component, initialType, catalogTypes]);

  // Use key to reset form state when component/initialType changes
  const formKey = component?.id || initialType || 'new';
  const [formData, setFormData] = useState<ComponentFormData>(initialFormData);

  // Reset form when the key changes (dialog opens with different data)
  const [lastKey, setLastKey] = useState(formKey);
  if (formKey !== lastKey) {
    setLastKey(formKey);
    setFormData(initialFormData);
  }

  const handleChange = (field: keyof ComponentFormData, value: string | null) => {
    setFormData((prev) => ({ ...prev, [field]: value }));
  };

  // Derive operation mode from current commands (for application-type components)
  const getAppRefMode = (): string => {
    const hasStart = formData.start_cmd === '@app:start';
    const hasStop = formData.stop_cmd === '@app:stop';
    if (hasStart && hasStop) return 'full';
    if (hasStart && !hasStop) return 'start-only';
    if (!hasStart && hasStop) return 'stop-only';
    return 'check-only';
  };

  // Handle operation mode change for application-type components
  const handleAppRefModeChange = (mode: string) => {
    setFormData((prev) => ({
      ...prev,
      check_cmd: '@app:check', // Always have check
      start_cmd: mode === 'full' || mode === 'start-only' ? '@app:start' : '',
      stop_cmd: mode === 'full' || mode === 'stop-only' ? '@app:stop' : '',
    }));
  };

  // When a referenced app is selected, auto-fill the commands and name
  const handleReferencedAppChange = (appRefId: string | null) => {
    const selectedApp = availableApps.find((a) => a.id === appRefId);
    if (selectedApp) {
      setFormData((prev) => ({
        ...prev,
        referenced_app_id: appRefId,
        name: prev.name || selectedApp.name.toLowerCase().replace(/\s+/g, '-'),
        display_name: prev.display_name || selectedApp.name,
        description: prev.description || `Synthetic view of ${selectedApp.name} application`,
        icon: 'folder',
        host: 'aggregate', // No specific host for app references
        // Default to full control - user can change via mode selector
        check_cmd: '@app:check',
        start_cmd: '@app:start',
        stop_cmd: '@app:stop',
      }));
    } else {
      setFormData((prev) => ({
        ...prev,
        referenced_app_id: null,
        check_cmd: '',
        start_cmd: '',
        stop_cmd: '',
      }));
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSave(formData);
  };

  const title = isCreating ? 'Add Component' : `Edit ${component?.name || 'Component'}`;

  return (
    <Dialog open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {isCreating
              ? 'Configure the new component properties'
              : 'Edit the component configuration'}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit}>
          <Tabs defaultValue="general" className="w-full">
            <TabsList className="grid w-full grid-cols-4">
              <TabsTrigger value="general">General</TabsTrigger>
              <TabsTrigger value="infra" disabled={formData.component_type === 'application'}>
                <MapPin className="h-3 w-3 mr-1" />
                Infrastructure
              </TabsTrigger>
              <TabsTrigger value="commands">Commands</TabsTrigger>
              <TabsTrigger value="advanced">Advanced</TabsTrigger>
            </TabsList>

            <TabsContent value="general" className="space-y-4 mt-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="name">Name *</Label>
                  <Input
                    id="name"
                    value={formData.name}
                    onChange={(e) => handleChange('name', e.target.value)}
                    placeholder="unique-component-name"
                    required
                  />
                  <p className="text-xs text-muted-foreground">
                    Technical identifier, must be unique
                  </p>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="display_name">Display Name</Label>
                  <Input
                    id="display_name"
                    value={formData.display_name}
                    onChange={(e) => handleChange('display_name', e.target.value)}
                    placeholder="Human-readable name"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label htmlFor="description">Description</Label>
                <Textarea
                  id="description"
                  value={formData.description}
                  onChange={(e) => handleChange('description', e.target.value)}
                  placeholder="What does this component do?"
                  rows={2}
                />
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="component_type">Type</Label>
                  <Select
                    value={formData.component_type}
                    onValueChange={(v) => {
                      handleChange('component_type', v);
                      // Reset referenced app when switching away from application type
                      if (v !== 'application') {
                        handleChange('referenced_app_id', null);
                      }
                      // Pre-fill default commands from catalog (only when creating or fields are empty)
                      const catalogType = catalogTypes.find((t) => t.type === v);
                      if (catalogType) {
                        handleChange('icon', catalogType.iconName);
                        setFormData((prev) => ({
                          ...prev,
                          component_type: v,
                          icon: catalogType.iconName,
                          check_cmd: prev.check_cmd || catalogType.defaultCheckCmd || '',
                          start_cmd: prev.start_cmd || catalogType.defaultStartCmd || '',
                          stop_cmd: prev.stop_cmd || catalogType.defaultStopCmd || '',
                        }));
                      }
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select type" />
                    </SelectTrigger>
                    <SelectContent>
                      <div className="px-2 pb-2">
                        <div className="relative">
                          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
                          <input
                            className="h-8 w-full rounded-md border border-input bg-background pl-7 pr-2 text-xs outline-none focus:ring-1 focus:ring-ring"
                            placeholder="Search types..."
                            onChange={(e) => {
                              // Filter is handled by SelectContent search
                              const input = e.target;
                              input.dataset.search = e.target.value;
                              // Force re-render of items
                              input.dispatchEvent(new Event('input', { bubbles: true }));
                            }}
                          />
                        </div>
                      </div>
                      {catalogTypes.map((t) => {
                        const Icon = t.icon;
                        return (
                          <SelectItem key={t.type} value={t.type}>
                            <div className="flex items-center gap-2">
                              <Icon className="h-4 w-4" style={{ color: t.color }} />
                              <span>{t.label}</span>
                              {t.category && (
                                <span className="text-[10px] text-muted-foreground ml-auto">
                                  {t.category}
                                </span>
                              )}
                            </div>
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="group">Group</Label>
                  <Select
                    value={formData.group_id || '_none'}
                    onValueChange={(v) => handleChange('group_id', v === '_none' ? null : v)}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="No group">
                        {formData.group_id ? (
                          <div className="flex items-center gap-2">
                            <div
                              className="w-3 h-3 rounded-full"
                              style={{ backgroundColor: groups?.find((g) => g.id === formData.group_id)?.color || '#6366F1' }}
                            />
                            {groups?.find((g) => g.id === formData.group_id)?.name || 'Unknown group'}
                          </div>
                        ) : (
                          'No group'
                        )}
                      </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="_none">No group</SelectItem>
                      {groups?.map((g) => (
                        <SelectItem key={g.id} value={g.id}>
                          <div className="flex items-center gap-2">
                            <div
                              className="w-3 h-3 rounded-full"
                              style={{ backgroundColor: g.color || '#6366F1' }}
                            />
                            {g.name}
                          </div>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>

              {/* Application Reference Selector - only show for application type */}
              {formData.component_type === 'application' && (
                <div className="space-y-2">
                  <Label>Referenced Application *</Label>
                  {availableApps.length === 0 ? (
                    <Alert>
                      <AlertCircle className="h-4 w-4" />
                      <AlertDescription>
                        No other applications available to reference. Create another application first.
                      </AlertDescription>
                    </Alert>
                  ) : (
                    <Select
                      value={formData.referenced_app_id || '_none'}
                      onValueChange={(v) => handleReferencedAppChange(v === '_none' ? null : v)}
                    >
                      <SelectTrigger>
                        <SelectValue>
                          {formData.referenced_app_id ? (
                            <div className="flex items-center gap-2">
                              <Folder className="h-4 w-4 text-blue-500" />
                              {availableApps.find((a) => a.id === formData.referenced_app_id)?.name ||
                                'Loading...'}
                            </div>
                          ) : (
                            <span className="text-muted-foreground">Select an application...</span>
                          )}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="_none">
                          <span className="text-muted-foreground">Select an application...</span>
                        </SelectItem>
                        {availableApps.map((app) => (
                          <SelectItem key={app.id} value={app.id}>
                            <div className="flex items-center gap-2">
                              <Folder className="h-4 w-4 text-blue-500" />
                              {app.name}
                            </div>
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                  <p className="text-xs text-muted-foreground">
                    This component will act as an aggregate view of the selected application.
                    Start/stop operations will cascade to all components in the referenced app.
                  </p>
                </div>
              )}

            </TabsContent>

            <TabsContent value="commands" className="space-y-4 mt-4">
              {formData.component_type === 'application' ? (
                <div className="space-y-4">
                  <Alert>
                    <Folder className="h-4 w-4" />
                    <AlertDescription>
                      <p className="font-medium mb-2">Application Reference</p>
                      <p className="text-sm text-muted-foreground">
                        This component represents the referenced application. Choose which operations are available:
                      </p>
                    </AlertDescription>
                  </Alert>

                  <div className="space-y-2">
                    <Label>Operation Mode</Label>
                    <Select
                      value={getAppRefMode()}
                      onValueChange={handleAppRefModeChange}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="full">
                          <div className="flex flex-col items-start">
                            <span className="font-medium">Full Control</span>
                            <span className="text-xs text-muted-foreground">Start, Stop, and Check</span>
                          </div>
                        </SelectItem>
                        <SelectItem value="start-only">
                          <div className="flex flex-col items-start">
                            <span className="font-medium">Start + Check</span>
                            <span className="text-xs text-muted-foreground">Can start and monitor, cannot stop</span>
                          </div>
                        </SelectItem>
                        <SelectItem value="stop-only">
                          <div className="flex flex-col items-start">
                            <span className="font-medium">Stop + Check</span>
                            <span className="text-xs text-muted-foreground">Can stop and monitor, cannot start</span>
                          </div>
                        </SelectItem>
                        <SelectItem value="check-only">
                          <div className="flex flex-col items-start">
                            <span className="font-medium">Check Only</span>
                            <span className="text-xs text-muted-foreground">Monitoring only, no control</span>
                          </div>
                        </SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Determines which actions are available for this application reference.
                    </p>
                  </div>

                  <div className="rounded-lg border p-3 bg-muted/50">
                    <p className="text-sm font-medium mb-2">Selected operations:</p>
                    <ul className="text-sm text-muted-foreground space-y-1">
                      <li className="flex items-center gap-2">
                        <span className="w-2 h-2 rounded-full bg-green-500" />
                        <strong>Check:</strong> Status aggregated from referenced app
                      </li>
                      {formData.start_cmd && (
                        <li className="flex items-center gap-2">
                          <span className="w-2 h-2 rounded-full bg-blue-500" />
                          <strong>Start:</strong> Starts entire app (DAG order)
                        </li>
                      )}
                      {formData.stop_cmd && (
                        <li className="flex items-center gap-2">
                          <span className="w-2 h-2 rounded-full bg-orange-500" />
                          <strong>Stop:</strong> Stops entire app (reverse DAG order)
                        </li>
                      )}
                    </ul>
                  </div>
                </div>
              ) : (
                <>
                  <div className="space-y-2">
                    <Label htmlFor="check_cmd">Check Command</Label>
                    <Textarea
                      id="check_cmd"
                      value={formData.check_cmd}
                      onChange={(e) => handleChange('check_cmd', e.target.value)}
                      placeholder="pgrep -f myapp || exit 1"
                      rows={2}
                      className="font-mono text-sm"
                    />
                    <p className="text-xs text-muted-foreground">
                      Exit 0 = running, non-zero = stopped/failed
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="start_cmd">Start Command</Label>
                    <Textarea
                      id="start_cmd"
                      value={formData.start_cmd}
                      onChange={(e) => handleChange('start_cmd', e.target.value)}
                      placeholder="systemctl start myapp"
                      rows={2}
                      className="font-mono text-sm"
                    />
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="stop_cmd">Stop Command</Label>
                    <Textarea
                      id="stop_cmd"
                      value={formData.stop_cmd}
                      onChange={(e) => handleChange('stop_cmd', e.target.value)}
                      placeholder="systemctl stop myapp"
                      rows={2}
                      className="font-mono text-sm"
                    />
                  </div>
                </>
              )}
            </TabsContent>

            <TabsContent value="infra" className="space-y-4 mt-4">
              {/* Primary Agent Selection */}
              <div className="space-y-3">
                <div className="flex items-center gap-2">
                  <Server className="h-4 w-4 text-blue-500" />
                  <h4 className="font-medium text-sm">Primary Agent</h4>
                </div>
                <p className="text-xs text-muted-foreground">
                  Select the agent that will execute commands for this component on the primary site.
                </p>
                <Select
                  value={formData.host || '_manual'}
                  onValueChange={(v) => {
                    if (v === '_manual') {
                      handleChange('host', '');
                    } else {
                      handleChange('host', v);
                    }
                  }}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select agent..." />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="_manual">Enter manually...</SelectItem>
                    {(() => {
                      // Group agents by gateway for clarity
                      type AgentItem = NonNullable<typeof agents>[number];
                      const grouped = new Map<string, AgentItem[]>();
                      const ungrouped: AgentItem[] = [];
                      for (const a of agents || []) {
                        if (a.gateway_name) {
                          const key = a.gateway_name;
                          if (!grouped.has(key)) grouped.set(key, []);
                          grouped.get(key)!.push(a);
                        } else {
                          ungrouped.push(a);
                        }
                      }
                      const items: React.ReactNode[] = [];
                      for (const [gwName, gwAgents] of grouped) {
                        items.push(
                          <div key={`gw-${gwName}`} className="px-2 py-1.5 text-xs font-semibold text-muted-foreground border-t first:border-t-0">
                            {gwName} {gwAgents[0]?.gateway_zone ? `(${gwAgents[0].gateway_zone})` : ''}
                          </div>
                        );
                        for (const a of gwAgents) {
                          items.push(
                            <SelectItem key={a.id} value={a.hostname}>
                              <div className="flex items-center gap-2 pl-2">
                                <div className={`w-2 h-2 rounded-full ${a.connected ? 'bg-green-500' : a.is_active ? 'bg-yellow-500' : 'bg-gray-400'}`} />
                                {a.hostname}
                              </div>
                            </SelectItem>
                          );
                        }
                      }
                      if (ungrouped.length > 0) {
                        if (grouped.size > 0) {
                          items.push(
                            <div key="gw-none" className="px-2 py-1.5 text-xs font-semibold text-muted-foreground border-t">
                              No gateway
                            </div>
                          );
                        }
                        for (const a of ungrouped) {
                          items.push(
                            <SelectItem key={a.id} value={a.hostname}>
                              <div className="flex items-center gap-2">
                                <div className={`w-2 h-2 rounded-full ${a.connected ? 'bg-green-500' : 'bg-gray-400'}`} />
                                {a.hostname}
                              </div>
                            </SelectItem>
                          );
                        }
                      }
                      return items;
                    })()}
                  </SelectContent>
                </Select>
                {(formData.host === '' || !agents?.find((a) => a.hostname === formData.host)) && (
                  <Input
                    value={formData.host}
                    onChange={(e) => handleChange('host', e.target.value)}
                    placeholder="hostname or IP address"
                  />
                )}
                {/* Show resolved agent info */}
                {formData.host && agents?.find((a) => a.hostname === formData.host) && (() => {
                  const agent = agents.find((a) => a.hostname === formData.host)!;
                  return (
                    <div className="rounded-md border p-2 bg-muted/30 text-xs flex items-center gap-3">
                      <div className={`w-2 h-2 rounded-full shrink-0 ${agent.connected ? 'bg-green-500' : 'bg-gray-400'}`} />
                      <span className="font-mono">{agent.hostname}</span>
                      {agent.gateway_name && (
                        <>
                          <span className="text-muted-foreground">via</span>
                          <span>{agent.gateway_name}</span>
                        </>
                      )}
                      {agent.gateway_zone && (
                        <span className="text-muted-foreground">({agent.gateway_zone})</span>
                      )}
                      <span className={agent.connected ? 'text-green-600' : 'text-gray-500'}>
                        {agent.connected ? 'Connected' : 'Disconnected'}
                      </span>
                    </div>
                  );
                })()}
              </div>

              {/* DR Site Overrides */}
              <div className="border-t pt-4 mt-4 space-y-3">
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-orange-500" />
                  <h4 className="font-medium text-sm">DR Site Overrides</h4>
                </div>
                <p className="text-xs text-muted-foreground">
                  Configure alternate agents and commands for failover sites. When the application runs on a DR site, these overrides replace the primary configuration.
                </p>
              </div>

              {/* Site overrides list */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <h4 className="font-medium text-sm">Site Overrides</h4>
                  {!isCreating && sites && sites.length > 0 && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        // Find first site that doesn't have an override yet
                        const configuredSiteIds = siteOverrides?.map(o => o.site_id) || [];
                        const availableSite = sites.find(s => !configuredSiteIds.includes(s.id));
                        if (availableSite) {
                          setEditingOverride({
                            siteId: availableSite.id,
                            agentId: null,
                            checkCmd: '',
                            startCmd: '',
                            stopCmd: '',
                            rebuildCmd: '',
                          });
                        }
                      }}
                    >
                      <Plus className="h-3 w-3 mr-1" />
                      Add Override
                    </Button>
                  )}
                </div>

                {isCreating ? (
                  <p className="text-sm text-muted-foreground italic">
                    Save the component first to configure site overrides.
                  </p>
                ) : !siteOverrides || siteOverrides.length === 0 ? (
                  <p className="text-sm text-muted-foreground italic">
                    No site overrides configured. Add an override to enable DR failover for this component.
                  </p>
                ) : (
                  <div className="space-y-2">
                    {siteOverrides.map((override) => (
                      <div
                        key={override.id}
                        className="rounded-lg border p-3 bg-background"
                      >
                        <div className="flex items-center justify-between mb-2">
                          <div className="flex items-center gap-2">
                            <div className={`w-2 h-2 rounded-full ${
                              sites?.find(s => s.id === override.site_id)?.site_type === 'dr'
                                ? 'bg-orange-500'
                                : 'bg-blue-500'
                            }`} />
                            <span className="font-medium text-sm">
                              {override.site_name || 'Unknown Site'}
                            </span>
                            <span className="text-xs text-muted-foreground px-1.5 py-0.5 bg-muted rounded">
                              {override.site_code}
                            </span>
                          </div>
                          <div className="flex items-center gap-1">
                            <Button
                              type="button"
                              variant="ghost"
                              size="sm"
                              onClick={() => setEditingOverride({
                                siteId: override.site_id,
                                agentId: override.agent_id_override,
                                checkCmd: override.check_cmd_override || '',
                                startCmd: override.start_cmd_override || '',
                                stopCmd: override.stop_cmd_override || '',
                                rebuildCmd: override.rebuild_cmd_override || '',
                              })}
                            >
                              Edit
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="sm"
                              className="text-destructive hover:text-destructive"
                              onClick={() => {
                                if (confirm('Remove this site override?')) {
                                  deleteOverride.mutate(override.site_id);
                                }
                              }}
                            >
                              <Trash2 className="h-3 w-3" />
                            </Button>
                          </div>
                        </div>
                        <div className="grid grid-cols-2 gap-2 text-sm">
                          <div>
                            <span className="text-muted-foreground">Agent: </span>
                            <span className="font-mono">
                              {override.agent_hostname || 'Same as default'}
                            </span>
                          </div>
                          {override.check_cmd_override && (
                            <div className="col-span-2">
                              <span className="text-muted-foreground">Check: </span>
                              <span className="font-mono text-xs">{override.check_cmd_override}</span>
                            </div>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {/* Edit override dialog inline */}
                {editingOverride && (
                  <div className="rounded-lg border-2 border-primary p-4 space-y-3 bg-muted/20">
                    <h5 className="font-medium text-sm">
                      {siteOverrides?.find(o => o.site_id === editingOverride.siteId)
                        ? 'Edit Override'
                        : 'New Override'}
                    </h5>

                    <div className="space-y-2">
                      <Label>Site</Label>
                      <Select
                        value={editingOverride.siteId}
                        onValueChange={(v) => setEditingOverride(prev => prev ? { ...prev, siteId: v } : null)}
                      >
                        <SelectTrigger>
                          <SelectValue>
                            {(() => {
                              const site = sites?.find(s => s.id === editingOverride.siteId);
                              if (!site) return 'Select site...';
                              return (
                                <div className="flex items-center gap-2">
                                  <div className={`w-2 h-2 rounded-full ${
                                    site.site_type === 'dr' ? 'bg-orange-500' :
                                    site.site_type === 'primary' ? 'bg-green-500' : 'bg-blue-500'
                                  }`} />
                                  {site.name} ({site.code})
                                </div>
                              );
                            })()}
                          </SelectValue>
                        </SelectTrigger>
                        <SelectContent>
                          {sites?.map((site) => (
                            <SelectItem key={site.id} value={site.id}>
                              <div className="flex items-center gap-2">
                                <div className={`w-2 h-2 rounded-full ${
                                  site.site_type === 'dr' ? 'bg-orange-500' :
                                  site.site_type === 'primary' ? 'bg-green-500' : 'bg-blue-500'
                                }`} />
                                {site.name} ({site.code}) - {site.site_type.toUpperCase()}
                              </div>
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>

                    <div className="space-y-2">
                      <Label>Override Agent</Label>
                      <Select
                        value={editingOverride.agentId || '_same'}
                        onValueChange={(v) => setEditingOverride(prev =>
                          prev ? { ...prev, agentId: v === '_same' ? null : v } : null
                        )}
                      >
                        <SelectTrigger>
                          <SelectValue>
                            {(() => {
                              if (!editingOverride.agentId) return <span className="text-muted-foreground">Use same agent as default</span>;
                              const agent = agents?.find(a => a.id === editingOverride.agentId);
                              if (!agent) return 'Select agent...';
                              return (
                                <div className="flex items-center gap-2">
                                  <div className={`w-2 h-2 rounded-full ${
                                    agent.status === 'active' ? 'bg-green-500' : 'bg-gray-400'
                                  }`} />
                                  {agent.hostname}
                                </div>
                              );
                            })()}
                          </SelectValue>
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="_same">
                            <span className="text-muted-foreground">Use same agent as default</span>
                          </SelectItem>
                          {agents?.map((a) => (
                            <SelectItem key={a.id} value={a.id}>
                              <div className="flex items-center gap-2">
                                <div className={`w-2 h-2 rounded-full ${
                                  a.status === 'active' ? 'bg-green-500' : 'bg-gray-400'
                                }`} />
                                {a.hostname}
                              </div>
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <p className="text-xs text-muted-foreground">
                        Agent to use when running on this site (typically the DR server)
                      </p>
                    </div>

                    <div className="space-y-2">
                      <Label>Check Command Override (optional)</Label>
                      <Input
                        value={editingOverride.checkCmd}
                        onChange={(e) => setEditingOverride(prev =>
                          prev ? { ...prev, checkCmd: e.target.value } : null
                        )}
                        placeholder="Leave empty to use default"
                        className="font-mono text-sm"
                      />
                    </div>

                    <div className="space-y-2">
                      <Label>Start Command Override (optional)</Label>
                      <Input
                        value={editingOverride.startCmd}
                        onChange={(e) => setEditingOverride(prev =>
                          prev ? { ...prev, startCmd: e.target.value } : null
                        )}
                        placeholder="Leave empty to use default"
                        className="font-mono text-sm"
                      />
                    </div>

                    <div className="space-y-2">
                      <Label>Stop Command Override (optional)</Label>
                      <Input
                        value={editingOverride.stopCmd}
                        onChange={(e) => setEditingOverride(prev =>
                          prev ? { ...prev, stopCmd: e.target.value } : null
                        )}
                        placeholder="Leave empty to use default"
                        className="font-mono text-sm"
                      />
                    </div>

                    <div className="space-y-2">
                      <Label>Rebuild Command Override (optional)</Label>
                      <Input
                        value={editingOverride.rebuildCmd}
                        onChange={(e) => setEditingOverride(prev =>
                          prev ? { ...prev, rebuildCmd: e.target.value } : null
                        )}
                        placeholder="Leave empty to use default"
                        className="font-mono text-sm"
                      />
                    </div>

                    <div className="flex gap-2 pt-2">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => setEditingOverride(null)}
                      >
                        Cancel
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        onClick={() => {
                          if (editingOverride) {
                            upsertOverride.mutate({
                              site_id: editingOverride.siteId,
                              agent_id_override: editingOverride.agentId,
                              check_cmd_override: editingOverride.checkCmd || null,
                              start_cmd_override: editingOverride.startCmd || null,
                              stop_cmd_override: editingOverride.stopCmd || null,
                              rebuild_cmd_override: editingOverride.rebuildCmd || null,
                            }, {
                              onSuccess: () => {
                                setEditingOverride(null);
                                refetchOverrides();
                              },
                            });
                          }
                        }}
                        disabled={upsertOverride.isPending}
                      >
                        {upsertOverride.isPending ? 'Saving...' : 'Save Override'}
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            </TabsContent>

            <TabsContent value="advanced" className="space-y-4 mt-4">
              {/* Timeouts and Intervals */}
              <div className="space-y-3">
                <h4 className="font-medium text-sm">Timing Configuration</h4>
                <div className="grid grid-cols-3 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="check_interval_seconds">Check Interval (sec)</Label>
                    <Input
                      id="check_interval_seconds"
                      type="number"
                      min={5}
                      max={3600}
                      value={formData.check_interval_seconds}
                      onChange={(e) => {
                        const val = parseInt(e.target.value, 10);
                        setFormData((prev) => ({
                          ...prev,
                          check_interval_seconds: isNaN(val) ? 30 : Math.max(5, val),
                        }));
                      }}
                    />
                    <p className="text-xs text-muted-foreground">
                      Health check frequency
                    </p>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="start_timeout_seconds">Start Timeout (sec)</Label>
                    <Input
                      id="start_timeout_seconds"
                      type="number"
                      min={10}
                      max={3600}
                      value={formData.start_timeout_seconds}
                      onChange={(e) => {
                        const val = parseInt(e.target.value, 10);
                        setFormData((prev) => ({
                          ...prev,
                          start_timeout_seconds: isNaN(val) ? 120 : Math.max(10, val),
                        }));
                      }}
                    />
                    <p className="text-xs text-muted-foreground">
                      Max wait for RUNNING
                    </p>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="stop_timeout_seconds">Stop Timeout (sec)</Label>
                    <Input
                      id="stop_timeout_seconds"
                      type="number"
                      min={10}
                      max={3600}
                      value={formData.stop_timeout_seconds}
                      onChange={(e) => {
                        const val = parseInt(e.target.value, 10);
                        setFormData((prev) => ({
                          ...prev,
                          stop_timeout_seconds: isNaN(val) ? 60 : Math.max(10, val),
                        }));
                      }}
                    />
                    <p className="text-xs text-muted-foreground">
                      Max wait for STOPPED
                    </p>
                  </div>
                </div>
              </div>

              {/* Optional flag */}
              <div className="flex items-center justify-between border-t pt-4">
                <div>
                  <Label>Optional Component</Label>
                  <p className="text-xs text-muted-foreground">
                    If enabled, failures won't block the application startup
                  </p>
                </div>
                <Switch
                  checked={formData.is_optional}
                  onCheckedChange={(checked: boolean) => {
                    setFormData((prev) => ({
                      ...prev,
                      is_optional: checked,
                    }));
                  }}
                />
              </div>

              {/* Icon */}
              <div className="space-y-2 border-t pt-4">
                <Label htmlFor="icon">Icon</Label>
                <Select value={formData.icon} onValueChange={(v) => handleChange('icon', v)}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select icon" />
                  </SelectTrigger>
                  <SelectContent>
                    {Object.entries(ICONS).map(([name, Icon]) => (
                      <SelectItem key={name} value={name}>
                        <div className="flex items-center gap-2">
                          <Icon className="h-4 w-4" />
                          {name}
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Cluster Section */}
              <div className="space-y-3 border-t pt-4">
                <div className="flex items-center justify-between">
                  <div>
                    <Label>Cluster Mode</Label>
                    <p className="text-xs text-muted-foreground">
                      Mark this component as a cluster with multiple nodes
                    </p>
                  </div>
                  <Switch
                    checked={!!formData.cluster_size && formData.cluster_size >= 2}
                    onCheckedChange={(checked: boolean) => {
                      if (!checked) {
                        setFormData((prev) => ({
                          ...prev,
                          cluster_size: null,
                          cluster_nodes: [],
                        }));
                      } else {
                        setFormData((prev) => ({
                          ...prev,
                          cluster_size: 2,
                          cluster_nodes: prev.cluster_nodes || [],
                        }));
                      }
                    }}
                  />
                </div>

                {formData.cluster_size && formData.cluster_size >= 2 && (
                  <>
                    <div className="space-y-2">
                      <Label htmlFor="cluster_size">Number of Nodes</Label>
                      <Input
                        id="cluster_size"
                        type="number"
                        min={2}
                        max={100}
                        value={formData.cluster_size}
                        onChange={(e) => {
                          const val = parseInt(e.target.value, 10);
                          setFormData((prev) => ({
                            ...prev,
                            cluster_size: isNaN(val) ? 2 : Math.max(2, val),
                          }));
                        }}
                      />
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="cluster_nodes">Server List (optional)</Label>
                      <Textarea
                        id="cluster_nodes"
                        placeholder="srv1.prod&#10;srv2.prod&#10;srv3.prod"
                        value={(formData.cluster_nodes || []).join('\n')}
                        onChange={(e) => {
                          const nodes = e.target.value
                            .split('\n')
                            .map((s) => s.trim())
                            .filter(Boolean);
                          setFormData((prev) => ({
                            ...prev,
                            cluster_nodes: nodes,
                          }));
                        }}
                        rows={3}
                        className="font-mono text-sm"
                      />
                      <p className="text-xs text-muted-foreground">
                        One server per line. The first entry is the primary node where commands are executed.
                      </p>
                    </div>
                  </>
                )}
              </div>
            </TabsContent>
          </Tabs>

          <DialogFooter className="mt-6">
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={!formData.name}>
              {isCreating ? 'Create Component' : 'Save Changes'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
