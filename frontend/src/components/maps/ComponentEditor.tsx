import { useState, useMemo, useEffect } from 'react';
import { Component, useComponentGroups, useApps } from '@/api/apps';
import { useAgents } from '@/api/reports';
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
import { COMPONENT_TYPES } from './ComponentPalette';
import { Database, Layers, Server, Globe, Cog, Clock, Box, Folder, AlertCircle } from 'lucide-react';
import { Alert, AlertDescription } from '@/components/ui/alert';

const ICONS: Record<string, React.ElementType> = {
  database: Database,
  layers: Layers,
  server: Server,
  globe: Globe,
  cog: Cog,
  clock: Clock,
  box: Box,
  folder: Folder,
};

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
  const { data: groups } = useComponentGroups(appId);
  const { data: agents } = useAgents();
  const { data: existingApps } = useApps();

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
        host: component.host || '',
        group_id: component.group_id,
        check_cmd: component.check_cmd || '',
        start_cmd: component.start_cmd || '',
        stop_cmd: component.stop_cmd || '',
        referenced_app_id: (component as Component & { referenced_app_id?: string }).referenced_app_id || null,
        cluster_size: component.cluster_size ?? null,
        cluster_nodes: component.cluster_nodes ?? [],
      };
    }
    if (initialType) {
      const typeInfo = COMPONENT_TYPES.find((t) => t.type === initialType);
      return {
        name: '',
        display_name: '',
        description: '',
        component_type: initialType,
        icon: typeInfo?.iconName || 'box',
        host: '',
        group_id: null,
        check_cmd: '',
        start_cmd: '',
        stop_cmd: '',
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
      referenced_app_id: null,
      cluster_size: null,
      cluster_nodes: [],
    };
  }, [component, initialType]);

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
        // Internal commands - backend interprets @app: prefix using referenced_app_id
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
            <TabsList className="grid w-full grid-cols-3">
              <TabsTrigger value="general">General</TabsTrigger>
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
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select type" />
                    </SelectTrigger>
                    <SelectContent>
                      {COMPONENT_TYPES.map((t) => (
                        <SelectItem key={t.type} value={t.type}>
                          <div className="flex items-center gap-2">
                            <t.icon className="h-4 w-4" style={{ color: t.color }} />
                            {t.label}
                          </div>
                        </SelectItem>
                      ))}
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
                      <SelectValue placeholder="No group" />
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
                      value={formData.referenced_app_id || ''}
                      onValueChange={(v) => handleReferencedAppChange(v || null)}
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select an application to reference" />
                      </SelectTrigger>
                      <SelectContent>
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

              <div className="space-y-2">
                <Label htmlFor="host">Host</Label>
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
                    <SelectValue placeholder="Select agent host" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="_manual">Enter manually...</SelectItem>
                    {agents?.map((a) => (
                      <SelectItem key={a.id} value={a.hostname}>
                        <div className="flex items-center gap-2">
                          <div
                            className={`w-2 h-2 rounded-full ${a.status === 'active' ? 'bg-green-500' : 'bg-gray-400'}`}
                          />
                          {a.hostname}
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                {(formData.host === '' || !agents?.find((a) => a.hostname === formData.host)) && (
                  <Input
                    value={formData.host}
                    onChange={(e) => handleChange('host', e.target.value)}
                    placeholder="hostname or IP address"
                    className="mt-2"
                  />
                )}
              </div>
            </TabsContent>

            <TabsContent value="commands" className="space-y-4 mt-4">
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
            </TabsContent>

            <TabsContent value="advanced" className="space-y-4 mt-4">
              <div className="space-y-2">
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
              <div className="space-y-3 border-t pt-4 mt-4">
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

              <p className="text-sm text-muted-foreground">
                Additional options like timeouts, optional flag, and environment variables can be
                configured after creation in the component details panel.
              </p>
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
