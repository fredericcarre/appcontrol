import { useState, useMemo } from 'react';
import { Component, useComponentGroups } from '@/api/apps';
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { COMPONENT_TYPES } from './ComponentPalette';
import { Database, Layers, Server, Globe, Cog, Clock, Box } from 'lucide-react';

const ICONS: Record<string, React.ElementType> = {
  database: Database,
  layers: Layers,
  server: Server,
  globe: Globe,
  cog: Cog,
  clock: Clock,
  box: Box,
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
                    onValueChange={(v) => handleChange('component_type', v)}
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
