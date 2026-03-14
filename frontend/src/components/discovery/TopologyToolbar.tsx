import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Loader2, CheckSquare, Square, Rocket, X, MapPin, ArrowLeft, Plus,
} from 'lucide-react';
import { useDiscoveryStore } from '@/stores/discovery';
import { useCreateDraft, useApplyDraft } from '@/api/discovery';
import { useSites } from '@/api/sites';
import type { CorrelatedService } from '@/api/discovery';
import type { ServiceEdits } from './TopologyMap.types';
import { CancelConfirmDialog } from './CancelConfirmDialog';
import { ConfidenceFilterBar } from './ConfidenceFilterBar';

export function TopologyToolbar() {
  const {
    correlationResult,
    enabledServiceIndices,
    serviceEdits,
    getEffectiveName,
    getEffectiveType,
    enableAll,
    disableAll,
    appName,
    setAppName,
    setCreatedAppId,
    setPhase,
    setCancelConfirmOpen,
    manualDependencies,
    selectedSiteId,
    setSelectedSiteId,
    addManualComponent,
  } = useDiscoveryStore();

  const createDraft = useCreateDraft();
  const applyDraft = useApplyDraft();
  const { data: sites } = useSites();
  const [creating, setCreating] = useState(false);
  const [addComponentOpen, setAddComponentOpen] = useState(false);
  const [newComponentName, setNewComponentName] = useState('');
  const [newComponentHost, setNewComponentHost] = useState('');
  const [newComponentType, setNewComponentType] = useState('service');

  const services = correlationResult?.services || [];
  const dependencies = correlationResult?.dependencies || [];
  const totalServices = services.length;
  const selectedCount = enabledServiceIndices.size;

  const handleCreateApp = async () => {
    if (!appName.trim() || selectedCount === 0 || !selectedSiteId) return;
    setCreating(true);

    try {
      // Build components from selected services
      const enabledIndices = [...enabledServiceIndices].sort((a, b) => a - b);
      const indexToTempId = new Map<number, string>();
      enabledIndices.forEach((idx, i) => {
        indexToTempId.set(idx, `svc-${i}`);
      });

      const components = enabledIndices.map((idx) => {
        const svc: CorrelatedService = services[idx];
        const edits: ServiceEdits | undefined = serviceEdits.get(idx);
        return {
          temp_id: indexToTempId.get(idx)!,
          name: getEffectiveName(idx),
          process_name: svc.process_name,
          host: svc.hostname,
          agent_id: svc.agent_id,
          listening_ports: svc.ports,
          component_type: getEffectiveType(idx),
          check_cmd: edits?.checkCmd ?? svc.command_suggestion?.check_cmd,
          start_cmd: edits?.startCmd ?? svc.command_suggestion?.start_cmd,
          stop_cmd: edits?.stopCmd ?? svc.command_suggestion?.stop_cmd,
          restart_cmd: edits?.restartCmd ?? svc.command_suggestion?.restart_cmd,
          command_confidence: svc.command_suggestion?.confidence,
          command_source: svc.command_suggestion?.source,
          config_files: svc.config_files,
          log_files: svc.log_files,
          matched_service: svc.matched_service,
        };
      });

      // Build dependencies between selected services (auto-detected)
      const depsToInclude = dependencies.filter(
        (d) =>
          d.from_service_index !== null &&
          d.from_service_index !== undefined &&
          indexToTempId.has(d.from_service_index) &&
          indexToTempId.has(d.to_service_index)
      );

      const draftDeps = depsToInclude.map((d) => ({
        from_temp_id: indexToTempId.get(d.from_service_index!)!,
        to_temp_id: indexToTempId.get(d.to_service_index)!,
        inferred_via: d.inferred_via,
      }));

      // Add manual dependencies (user-created)
      const manualDepsToInclude = manualDependencies.filter(
        (md) => indexToTempId.has(md.from) && indexToTempId.has(md.to)
      );

      for (const md of manualDepsToInclude) {
        draftDeps.push({
          from_temp_id: indexToTempId.get(md.from)!,
          to_temp_id: indexToTempId.get(md.to)!,
          inferred_via: 'manual',
        });
      }

      // Create draft
      const draft = await createDraft.mutateAsync({
        name: appName.trim(),
        site_id: selectedSiteId || undefined,
        components,
        dependencies: draftDeps,
      });

      // Apply draft immediately
      const result = await applyDraft.mutateAsync(draft.id);

      setCreatedAppId(result.app_id || draft.id);
      setPhase('done');
    } catch (err) {
      console.error('Failed to create application:', err);
    } finally {
      setCreating(false);
    }
  };

  return (
    <>
      <div className="absolute top-3 left-1/2 -translate-x-1/2 z-10 flex flex-col gap-2">
        {/* Top row: Confidence filters */}
        <div className="flex justify-center">
          <div className="bg-card/95 backdrop-blur-sm border border-border rounded-lg shadow-lg px-3 py-1.5">
            <ConfidenceFilterBar />
          </div>
        </div>

        {/* Bottom row: Actions */}
        <div className="flex items-center gap-2 bg-card/95 backdrop-blur-sm border border-border rounded-lg shadow-lg px-3 py-2">
          {/* Back to scan */}
          <div className="pr-2 border-r border-border">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs gap-1 text-muted-foreground hover:text-foreground"
              onClick={() => setPhase('scan')}
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              Back
            </Button>
          </div>

          {/* Cancel button */}
          <div className="pr-2 border-r border-border">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs gap-1 text-muted-foreground hover:text-destructive"
              onClick={() => setCancelConfirmOpen(true)}
            >
              <X className="h-3.5 w-3.5" />
              Cancel
            </Button>
          </div>

          {/* Selection */}
          <div className="flex items-center gap-1.5 pr-2 border-r border-border">
            <button onClick={enableAll} className="p-1 rounded hover:bg-accent" title="Select all">
              <CheckSquare className="h-3.5 w-3.5 text-muted-foreground" />
            </button>
            <button onClick={disableAll} className="p-1 rounded hover:bg-accent" title="Deselect all">
              <Square className="h-3.5 w-3.5 text-muted-foreground" />
            </button>
            <Badge variant="secondary" className="text-[10px] px-1.5 py-0 font-mono">
              {selectedCount}/{totalServices}
            </Badge>
          </div>

          {/* Add component manually */}
          <div className="pr-2 border-r border-border">
            <Button
              variant="outline"
              size="sm"
              className="h-7 text-xs gap-1.5"
              onClick={() => setAddComponentOpen(true)}
            >
              <Plus className="h-3.5 w-3.5" />
              Add
            </Button>
          </div>

          {/* Site selector */}
          <div className="pr-2 border-r border-border">
            <Select
              value={selectedSiteId || ''}
              onValueChange={(v) => setSelectedSiteId(v || null)}
            >
              <SelectTrigger className="h-7 w-36 text-xs">
                <div className="flex items-center gap-1.5">
                  <MapPin className="h-3 w-3 text-muted-foreground" />
                  {selectedSiteId ? (
                    <span>{sites?.find(s => s.id === selectedSiteId)?.name || 'Site'}</span>
                  ) : (
                    <span className="text-muted-foreground">Select site...</span>
                  )}
                </div>
              </SelectTrigger>
              <SelectContent>
                {sites?.map((site) => (
                  <SelectItem key={site.id} value={site.id}>
                    <span className="flex items-center gap-1.5">
                      <span>{site.name}</span>
                      <span className="text-[10px] text-muted-foreground">({site.code})</span>
                    </span>
                  </SelectItem>
                ))}
                {(!sites || sites.length === 0) && (
                  <div className="px-2 py-1.5 text-xs text-muted-foreground">No sites available</div>
                )}
              </SelectContent>
            </Select>
          </div>

          {/* App name + Create */}
          <div className="flex items-center gap-2">
            <Input
              value={appName}
              onChange={(e) => setAppName(e.target.value)}
              placeholder="Application name..."
              className="h-7 w-40 text-xs"
            />
            <Button
              size="sm"
              className="h-7 text-xs gap-1.5"
              disabled={!appName.trim() || selectedCount === 0 || !selectedSiteId || creating}
              onClick={handleCreateApp}
            >
              {creating ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Rocket className="h-3.5 w-3.5" />
              )}
              Create
            </Button>
          </div>
        </div>
      </div>
      <CancelConfirmDialog />

      {/* Add Component Dialog */}
      <Dialog open={addComponentOpen} onOpenChange={setAddComponentOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Add Component</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="component-name">Name</Label>
              <Input
                id="component-name"
                placeholder="e.g. MyDatabase"
                value={newComponentName}
                onChange={(e) => setNewComponentName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="component-host">Hostname</Label>
              <Input
                id="component-host"
                placeholder="e.g. server01"
                value={newComponentHost}
                onChange={(e) => setNewComponentHost(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="component-type">Type</Label>
              <Select value={newComponentType} onValueChange={setNewComponentType}>
                <SelectTrigger>
                  {newComponentType || 'Select type...'}
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="database">Database</SelectItem>
                  <SelectItem value="middleware">Middleware</SelectItem>
                  <SelectItem value="appserver">App Server</SelectItem>
                  <SelectItem value="webfront">Web Frontend</SelectItem>
                  <SelectItem value="service">Service</SelectItem>
                  <SelectItem value="batch">Batch Job</SelectItem>
                  <SelectItem value="cache">Cache</SelectItem>
                  <SelectItem value="loadbalancer">Load Balancer</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddComponentOpen(false)}>
              Cancel
            </Button>
            <Button
              onClick={() => {
                if (newComponentName.trim()) {
                  addManualComponent(
                    newComponentName.trim(),
                    newComponentHost.trim() || 'manual',
                    newComponentType
                  );
                  setNewComponentName('');
                  setNewComponentHost('');
                  setNewComponentType('service');
                  setAddComponentOpen(false);
                }
              }}
              disabled={!newComponentName.trim()}
            >
              Add
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
