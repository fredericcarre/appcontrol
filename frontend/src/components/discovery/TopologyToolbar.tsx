import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Loader2, CheckSquare, Square, Rocket,
} from 'lucide-react';
import { useDiscoveryStore } from '@/stores/discovery';
import { useCreateDraft, useApplyDraft } from '@/api/discovery';
import type { CorrelatedService } from '@/api/discovery';
import type { ServiceEdits } from './TopologyMap.types';

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
  } = useDiscoveryStore();

  const createDraft = useCreateDraft();
  const applyDraft = useApplyDraft();
  const [creating, setCreating] = useState(false);

  const services = correlationResult?.services || [];
  const dependencies = correlationResult?.dependencies || [];
  const totalServices = services.length;
  const selectedCount = enabledServiceIndices.size;

  const handleCreateApp = async () => {
    if (!appName.trim() || selectedCount === 0) return;
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

      // Build dependencies between selected services
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

      // Create draft
      const draft = await createDraft.mutateAsync({
        name: appName.trim(),
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
    <div className="absolute top-3 left-1/2 -translate-x-1/2 z-10">
      <div className="flex items-center gap-2 bg-card/95 backdrop-blur-sm border border-border rounded-lg shadow-lg px-3 py-2">
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

        {/* App name + Create */}
        <div className="flex items-center gap-2">
          <Input
            value={appName}
            onChange={(e) => setAppName(e.target.value)}
            placeholder="Application name..."
            className="h-7 w-48 text-xs"
          />
          <Button
            size="sm"
            className="h-7 text-xs gap-1.5"
            disabled={!appName.trim() || selectedCount === 0 || creating}
            onClick={handleCreateApp}
          >
            {creating ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Rocket className="h-3.5 w-3.5" />
            )}
            Create Application
          </Button>
        </div>
      </div>
    </div>
  );
}
