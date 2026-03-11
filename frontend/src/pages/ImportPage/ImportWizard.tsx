import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Upload, Server, CheckCircle2, AlertTriangle, ArrowLeft, ArrowRight, Loader2,
  FileJson, FileCode, Shield, Check, X, HelpCircle
} from 'lucide-react';
import {
  useImportPreview,
  useImportExecute,
  ImportPreviewResponse,
  MappingConfig,
  ComponentResolution,
  ComponentResolutionStatus,
  AvailableAgent,
  ConflictAction,
} from '@/api/import';
import { useGateways, Gateway } from '@/api/gateways';
import { JsonEditor, JsonError } from '@/components/JsonEditor';

// ═══════════════════════════════════════════════════════════════════════════
// Wizard Steps
// ═══════════════════════════════════════════════════════════════════════════

type WizardStep = 'upload' | 'gateway' | 'resolution' | 'confirm';

interface WizardState {
  // Step 1: Upload
  content: string;
  format: 'json' | 'yaml';
  jsonError: JsonError | null;
  // Step 2: Gateway
  selectedGatewayIds: string[];
  selectedDrGatewayIds: string[];
  // Step 3: Resolution
  preview: ImportPreviewResponse | null;
  manualMappings: Record<string, string>; // component_name -> agent_id
  // Step 4: Confirm
  profileName: string;
  drProfileName: string;
  enableDr: boolean;
  autoFailover: boolean;
  // Conflict handling
  conflictAction: ConflictAction;
  newName: string;
}

const initialState: WizardState = {
  content: '',
  format: 'json',
  jsonError: null,
  selectedGatewayIds: [],
  selectedDrGatewayIds: [],
  preview: null,
  manualMappings: {},
  profileName: 'prod',
  drProfileName: 'dr',
  enableDr: false,
  autoFailover: false,
  conflictAction: 'fail',
  newName: '',
};

export default function ImportWizard() {
  const navigate = useNavigate();
  const [step, setStep] = useState<WizardStep>('upload');
  const [state, setState] = useState<WizardState>(initialState);

  const { data: gateways = [] } = useGateways();
  const previewMutation = useImportPreview();
  const executeMutation = useImportExecute();

  const updateState = useCallback((updates: Partial<WizardState>) => {
    setState((prev) => ({ ...prev, ...updates }));
  }, []);

  const steps: { key: WizardStep; label: string; icon: React.ReactNode }[] = [
    { key: 'upload', label: 'Upload', icon: <Upload className="h-4 w-4" /> },
    { key: 'gateway', label: 'Gateways', icon: <Server className="h-4 w-4" /> },
    { key: 'resolution', label: 'Resolution', icon: <CheckCircle2 className="h-4 w-4" /> },
    { key: 'confirm', label: 'Confirm', icon: <Shield className="h-4 w-4" /> },
  ];

  const currentStepIndex = steps.findIndex((s) => s.key === step);
  const progress = ((currentStepIndex + 1) / steps.length) * 100;

  // ─────────────────────────────────────────────────────────────────────────
  // Navigation handlers
  // ─────────────────────────────────────────────────────────────────────────

  const canProceed = (): boolean => {
    switch (step) {
      case 'upload':
        // For JSON format, also check for validation errors
        if (state.format === 'json' && state.jsonError) return false;
        return state.content.trim().length > 0;
      case 'gateway':
        return state.selectedGatewayIds.length > 0;
      case 'resolution':
        return state.preview ? isAllResolved(state.preview, state.manualMappings) : false;
      case 'confirm': {
        if (!state.profileName.trim()) return false;
        // If there's a conflict, must choose rename or update
        if (state.preview?.existing_application) {
          if (state.conflictAction === 'fail') return false;
          if (state.conflictAction === 'rename' && !state.newName.trim()) return false;
        }
        return true;
      }
      default:
        return false;
    }
  };

  const handleNext = async () => {
    if (step === 'upload') {
      setStep('gateway');
    } else if (step === 'gateway') {
      // Trigger preview
      previewMutation.mutate(
        {
          content: state.content,
          format: state.format,
          gateway_ids: state.selectedGatewayIds,
          dr_gateway_ids: state.enableDr ? state.selectedDrGatewayIds : undefined,
        },
        {
          onSuccess: (data) => {
            updateState({ preview: data });
            setStep('resolution');
          },
        }
      );
    } else if (step === 'resolution') {
      setStep('confirm');
    } else if (step === 'confirm') {
      // Execute import
      const mappings = buildMappings(state.preview!, state.manualMappings);
      const drMappings = state.enableDr && state.preview?.dr_suggestions
        ? buildDrMappings(state.preview.dr_suggestions, state.manualMappings)
        : undefined;

      executeMutation.mutate(
        {
          content: state.content,
          format: state.format,
          profile: {
            name: state.profileName,
            profile_type: 'primary',
            gateway_ids: state.selectedGatewayIds,
            mappings,
          },
          dr_profile: drMappings
            ? {
                name: state.drProfileName,
                profile_type: 'dr',
                gateway_ids: state.selectedDrGatewayIds,
                auto_failover: state.autoFailover,
                mappings: drMappings,
              }
            : undefined,
          conflict_action: state.preview?.existing_application ? state.conflictAction : undefined,
          new_name: state.conflictAction === 'rename' ? state.newName : undefined,
        },
        {
          onSuccess: (data) => {
            navigate(`/apps/${data.application_id}`);
          },
        }
      );
    }
  };

  const handleBack = () => {
    const idx = currentStepIndex;
    if (idx > 0) {
      setStep(steps[idx - 1].key);
    }
  };

  // ─────────────────────────────────────────────────────────────────────────
  // Render
  // ─────────────────────────────────────────────────────────────────────────

  return (
    <div className="container mx-auto p-6 max-w-4xl">
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Import Application Map</h1>
        <p className="text-muted-foreground">
          Import an application map with gateway resolution and binding profiles.
        </p>
      </div>

      {/* Progress bar */}
      <div className="mb-8">
        <Progress value={progress} className="h-2" />
        <div className="flex justify-between mt-2">
          {steps.map((s, idx) => (
            <div
              key={s.key}
              className={`flex items-center gap-1 text-sm ${
                idx <= currentStepIndex ? 'text-primary' : 'text-muted-foreground'
              }`}
            >
              {s.icon}
              <span className="hidden sm:inline">{s.label}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Step content */}
      <Card className="mb-6">
        <CardContent className="pt-6">
          {step === 'upload' && (
            <UploadStep
              content={state.content}
              format={state.format}
              onContentChange={(content) => updateState({ content })}
              onFormatChange={(format) => updateState({ format })}
              onJsonErrorChange={(jsonError) => updateState({ jsonError })}
            />
          )}
          {step === 'gateway' && (
            <GatewayStep
              gateways={gateways}
              selectedGatewayIds={state.selectedGatewayIds}
              selectedDrGatewayIds={state.selectedDrGatewayIds}
              enableDr={state.enableDr}
              onGatewayIdsChange={(ids) => updateState({ selectedGatewayIds: ids })}
              onDrGatewayIdsChange={(ids) => updateState({ selectedDrGatewayIds: ids })}
              onEnableDrChange={(enabled) => updateState({ enableDr: enabled })}
            />
          )}
          {step === 'resolution' && state.preview && (
            <ResolutionStep
              preview={state.preview}
              manualMappings={state.manualMappings}
              onMappingChange={(compName, agentId) =>
                updateState({
                  manualMappings: { ...state.manualMappings, [compName]: agentId },
                })
              }
            />
          )}
          {step === 'confirm' && state.preview && (
            <ConfirmStep
              preview={state.preview}
              profileName={state.profileName}
              drProfileName={state.drProfileName}
              enableDr={state.enableDr}
              autoFailover={state.autoFailover}
              conflictAction={state.conflictAction}
              newName={state.newName}
              onProfileNameChange={(name) => updateState({ profileName: name })}
              onDrProfileNameChange={(name) => updateState({ drProfileName: name })}
              onAutoFailoverChange={(enabled) => updateState({ autoFailover: enabled })}
              onConflictActionChange={(action) => updateState({ conflictAction: action })}
              onNewNameChange={(name) => updateState({ newName: name })}
            />
          )}
        </CardContent>
      </Card>

      {/* Error display */}
      {(previewMutation.isError || executeMutation.isError) && (
        <Card className="mb-6 border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800">
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-red-700 dark:text-red-300">
              <AlertTriangle className="h-5 w-5" />
              <span>
                {previewMutation.error?.message || executeMutation.error?.message || 'An error occurred'}
              </span>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Navigation buttons */}
      <div className="flex justify-between">
        <Button variant="outline" onClick={handleBack} disabled={currentStepIndex === 0}>
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back
        </Button>
        <Button
          onClick={handleNext}
          disabled={!canProceed() || previewMutation.isPending || executeMutation.isPending}
        >
          {(previewMutation.isPending || executeMutation.isPending) && (
            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
          )}
          {step === 'confirm' ? 'Import' : 'Next'}
          {step !== 'confirm' && <ArrowRight className="h-4 w-4 ml-2" />}
        </Button>
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 1: Upload
// ═══════════════════════════════════════════════════════════════════════════

interface UploadStepProps {
  content: string;
  format: 'json' | 'yaml';
  onContentChange: (content: string) => void;
  onFormatChange: (format: 'json' | 'yaml') => void;
  onJsonErrorChange: (error: JsonError | null) => void;
}

function UploadStep({ content, format, onContentChange, onFormatChange, onJsonErrorChange }: UploadStepProps) {
  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const text = ev.target?.result as string;
      onContentChange(text);
      // Auto-detect format
      if (file.name.endsWith('.json')) {
        onFormatChange('json');
      } else if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
        onFormatChange('yaml');
      }
    };
    reader.readAsText(file);
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Upload or paste your map file</h3>
        <p className="text-muted-foreground text-sm">
          Supports JSON (v4) and YAML (v3 legacy) formats.
        </p>
      </div>

      <div className="flex gap-4">
        <Button
          variant={format === 'json' ? 'default' : 'outline'}
          onClick={() => onFormatChange('json')}
          className="flex-1"
        >
          <FileJson className="h-4 w-4 mr-2" />
          JSON (v4)
        </Button>
        <Button
          variant={format === 'yaml' ? 'default' : 'outline'}
          onClick={() => onFormatChange('yaml')}
          className="flex-1"
        >
          <FileCode className="h-4 w-4 mr-2" />
          YAML (Legacy)
        </Button>
      </div>

      <div>
        <label className="block text-sm font-medium mb-2">Upload file</label>
        <input
          type="file"
          accept=".json,.yaml,.yml"
          onChange={handleFileUpload}
          className="block w-full text-sm file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90"
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-2">Or paste content</label>
        {format === 'json' ? (
          <JsonEditor
            value={content}
            onChange={onContentChange}
            onValidationChange={onJsonErrorChange}
            placeholder={'{\n  "application": {\n    "name": "My App",\n    "components": []\n  }\n}'}
            height="350px"
          />
        ) : (
          <textarea
            value={content}
            onChange={(e) => onContentChange(e.target.value)}
            placeholder={'application:\n  name: My App\n  components:\n    - name: component1\n      ...'}
            className="w-full h-64 px-3 py-2 border rounded-md bg-background text-sm font-mono"
          />
        )}
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 2: Gateway Selection
// ═══════════════════════════════════════════════════════════════════════════

interface GatewayStepProps {
  gateways: Gateway[];
  selectedGatewayIds: string[];
  selectedDrGatewayIds: string[];
  enableDr: boolean;
  onGatewayIdsChange: (ids: string[]) => void;
  onDrGatewayIdsChange: (ids: string[]) => void;
  onEnableDrChange: (enabled: boolean) => void;
}

function GatewayStep({
  gateways,
  selectedGatewayIds,
  selectedDrGatewayIds,
  enableDr,
  onGatewayIdsChange,
  onDrGatewayIdsChange,
  onEnableDrChange,
}: GatewayStepProps) {
  const toggleGateway = (id: string) => {
    if (selectedGatewayIds.includes(id)) {
      onGatewayIdsChange(selectedGatewayIds.filter((gid) => gid !== id));
    } else {
      onGatewayIdsChange([...selectedGatewayIds, id]);
    }
  };

  const toggleDrGateway = (id: string) => {
    if (selectedDrGatewayIds.includes(id)) {
      onDrGatewayIdsChange(selectedDrGatewayIds.filter((gid) => gid !== id));
    } else {
      onDrGatewayIdsChange([...selectedDrGatewayIds, id]);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Select Primary Gateways</h3>
        <p className="text-muted-foreground text-sm mb-4">
          Choose the gateways that host the agents for this application.
        </p>
        <div className="grid gap-2">
          {gateways.map((gw) => (
            <div
              key={gw.id}
              onClick={() => toggleGateway(gw.id)}
              className={`flex items-center justify-between p-3 border rounded-md cursor-pointer hover:bg-accent ${
                selectedGatewayIds.includes(gw.id) ? 'border-primary bg-primary/5' : ''
              }`}
            >
              <div className="flex items-center gap-3">
                <Server className="h-5 w-5 text-muted-foreground" />
                <div>
                  <div className="font-medium">{gw.name}</div>
                  <div className="text-sm text-muted-foreground">{gw.zone}</div>
                </div>
              </div>
              {selectedGatewayIds.includes(gw.id) && <Check className="h-5 w-5 text-primary" />}
            </div>
          ))}
        </div>
      </div>

      <div className="border-t pt-4">
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={enableDr}
            onChange={(e) => onEnableDrChange(e.target.checked)}
            className="rounded"
          />
          <span className="font-medium">Configure DR profile</span>
        </label>
        <p className="text-muted-foreground text-sm mt-1">
          Create a disaster recovery profile with separate gateway bindings.
        </p>
      </div>

      {enableDr && (
        <div>
          <h3 className="text-lg font-medium mb-2">Select DR Gateways</h3>
          <div className="grid gap-2">
            {gateways.map((gw) => (
              <div
                key={gw.id}
                onClick={() => toggleDrGateway(gw.id)}
                className={`flex items-center justify-between p-3 border rounded-md cursor-pointer hover:bg-accent ${
                  selectedDrGatewayIds.includes(gw.id) ? 'border-orange-500 bg-orange-500/5' : ''
                }`}
              >
                <div className="flex items-center gap-3">
                  <Shield className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <div className="font-medium">{gw.name}</div>
                    <div className="text-sm text-muted-foreground">{gw.zone}</div>
                  </div>
                </div>
                {selectedDrGatewayIds.includes(gw.id) && <Check className="h-5 w-5 text-orange-500" />}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 3: Resolution
// ═══════════════════════════════════════════════════════════════════════════

interface ResolutionStepProps {
  preview: ImportPreviewResponse;
  manualMappings: Record<string, string>;
  onMappingChange: (componentName: string, agentId: string) => void;
}

function ResolutionStep({ preview, manualMappings, onMappingChange }: ResolutionStepProps) {
  const components = preview.components || [];
  const resolved = components.filter((c) => c.resolution.status === 'resolved');
  const multiple = components.filter((c) => c.resolution.status === 'multiple');
  const unresolved = components.filter(
    (c) => c.resolution.status === 'unresolved' || c.resolution.status === 'no_host'
  );

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Host Resolution Preview</h3>
        <p className="text-muted-foreground text-sm">
          {preview.application_name} - {preview.component_count} components
        </p>
      </div>

      {/* Summary */}
      <div className="flex gap-4">
        <div className="flex-1 p-3 bg-green-50 dark:bg-green-950 rounded-md">
          <div className="flex items-center gap-2 text-green-700 dark:text-green-300">
            <Check className="h-5 w-5" />
            <span className="font-medium">{resolved.length} Resolved</span>
          </div>
        </div>
        <div className="flex-1 p-3 bg-amber-50 dark:bg-amber-950 rounded-md">
          <div className="flex items-center gap-2 text-amber-700 dark:text-amber-300">
            <HelpCircle className="h-5 w-5" />
            <span className="font-medium">{multiple.length} Multiple</span>
          </div>
        </div>
        <div className="flex-1 p-3 bg-red-50 dark:bg-red-950 rounded-md">
          <div className="flex items-center gap-2 text-red-700 dark:text-red-300">
            <X className="h-5 w-5" />
            <span className="font-medium">{unresolved.length} Unresolved</span>
          </div>
        </div>
      </div>

      {/* Components list */}
      <div className="space-y-2 max-h-96 overflow-y-auto">
        {(preview.components || []).map((comp) => (
          <ComponentResolutionRow
            key={comp.name}
            component={comp}
            availableAgents={preview.available_agents}
            selectedAgentId={manualMappings[comp.name]}
            onSelectAgent={(agentId) => onMappingChange(comp.name, agentId)}
          />
        ))}
      </div>

      {/* Warnings */}
      {(preview.warnings || []).length > 0 && (
        <div className="p-3 bg-amber-50 dark:bg-amber-950 rounded-md">
          <div className="flex items-center gap-2 text-amber-700 dark:text-amber-300 mb-2">
            <AlertTriangle className="h-4 w-4" />
            <span className="font-medium">Warnings</span>
          </div>
          <ul className="text-sm text-amber-600 dark:text-amber-400 list-disc list-inside">
            {(preview.warnings || []).map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

interface ComponentResolutionRowProps {
  component: ComponentResolution;
  availableAgents: AvailableAgent[];
  selectedAgentId?: string;
  onSelectAgent: (agentId: string) => void;
}

function ComponentResolutionRow({
  component,
  availableAgents,
  selectedAgentId,
  onSelectAgent,
}: ComponentResolutionRowProps) {
  const { resolution } = component;
  const needsSelection = resolution.status === 'multiple' || resolution.status === 'unresolved' || resolution.status === 'no_host';

  const getStatusIcon = () => {
    if (selectedAgentId && needsSelection) {
      return <Check className="h-4 w-4 text-green-600" />;
    }
    switch (resolution.status) {
      case 'resolved':
        return <Check className="h-4 w-4 text-green-600" />;
      case 'multiple':
        return <HelpCircle className="h-4 w-4 text-amber-600" />;
      default:
        return <X className="h-4 w-4 text-red-600" />;
    }
  };

  const getStatusText = () => {
    if (selectedAgentId && needsSelection) {
      const agent = (availableAgents || []).find((a) => a.agent_id === selectedAgentId);
      return agent ? `${agent.hostname} (manual)` : 'Selected';
    }
    switch (resolution.status) {
      case 'resolved':
        return `${resolution.agent_hostname} (${resolution.resolved_via})`;
      case 'multiple':
        return `${resolution.candidates.length} matches - select one`;
      case 'unresolved':
        return 'No match found - select manually';
      case 'no_host':
        return 'No host specified - select manually';
    }
  };

  return (
    <div className="flex items-center gap-4 p-3 border rounded-md">
      <div className="flex-1">
        <div className="flex items-center gap-2">
          {getStatusIcon()}
          <span className="font-medium">{component.name}</span>
          <span className="text-sm text-muted-foreground">({component.component_type})</span>
        </div>
        <div className="text-sm text-muted-foreground mt-1">
          {component.host ? `Host: ${component.host}` : 'No host'} &rarr; {getStatusText()}
        </div>
      </div>
      {needsSelection && (
        <select
          value={selectedAgentId || ''}
          onChange={(e) => onSelectAgent(e.target.value)}
          className="px-2 py-1 border rounded-md bg-background text-sm"
        >
          <option value="">Select agent...</option>
          {resolution.status === 'multiple' &&
            (resolution.candidates || []).map((c) => (
              <option key={c.agent_id} value={c.agent_id}>
                {c.hostname} ({c.matched_via})
              </option>
            ))}
          {(resolution.status === 'unresolved' || resolution.status === 'no_host') &&
            (availableAgents || []).map((a) => (
              <option key={a.agent_id} value={a.agent_id}>
                {a.hostname} {a.gateway_name ? `(${a.gateway_name})` : ''}
              </option>
            ))}
        </select>
      )}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 4: Confirm
// ═══════════════════════════════════════════════════════════════════════════

interface ConfirmStepProps {
  preview: ImportPreviewResponse;
  profileName: string;
  drProfileName: string;
  enableDr: boolean;
  autoFailover: boolean;
  conflictAction: ConflictAction;
  newName: string;
  onProfileNameChange: (name: string) => void;
  onDrProfileNameChange: (name: string) => void;
  onAutoFailoverChange: (enabled: boolean) => void;
  onConflictActionChange: (action: ConflictAction) => void;
  onNewNameChange: (name: string) => void;
}

function ConfirmStep({
  preview,
  profileName,
  drProfileName,
  enableDr,
  autoFailover,
  conflictAction,
  newName,
  onProfileNameChange,
  onDrProfileNameChange,
  onAutoFailoverChange,
  onConflictActionChange,
  onNewNameChange,
}: ConfirmStepProps) {
  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Confirm Import</h3>
        <p className="text-muted-foreground text-sm">
          Review your configuration before importing.
        </p>
      </div>

      {/* Conflict warning */}
      {preview.existing_application && (
        <div className="p-4 border border-amber-300 dark:border-amber-700 rounded-md bg-amber-50 dark:bg-amber-950">
          <div className="flex items-center gap-2 mb-3">
            <AlertTriangle className="h-5 w-5 text-amber-600" />
            <span className="font-medium text-amber-800 dark:text-amber-200">
              Application "{preview.existing_application.name}" already exists
            </span>
          </div>
          <p className="text-sm text-amber-700 dark:text-amber-300 mb-3">
            Created with {preview.existing_application.component_count} components. Choose how to proceed:
          </p>
          <div className="space-y-2">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                name="conflict"
                checked={conflictAction === 'rename'}
                onChange={() => onConflictActionChange('rename')}
                className="text-amber-600"
              />
              <span className="text-sm">Import with a new name</span>
            </label>
            {conflictAction === 'rename' && (
              <input
                type="text"
                value={newName}
                onChange={(e) => onNewNameChange(e.target.value)}
                placeholder="Enter new application name"
                className="w-full px-3 py-2 border rounded-md bg-background text-sm ml-6"
              />
            )}
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                name="conflict"
                checked={conflictAction === 'update'}
                onChange={() => onConflictActionChange('update')}
                className="text-amber-600"
              />
              <span className="text-sm">Update existing application (replace components and profiles)</span>
            </label>
          </div>
        </div>
      )}

      <div className="grid gap-4">
        <div className="p-4 border rounded-md">
          <div className="font-medium mb-2">Application</div>
          <div className="text-sm text-muted-foreground">
            {conflictAction === 'rename' && newName ? newName : preview.application_name}
          </div>
          <div className="text-sm text-muted-foreground">{preview.component_count} components</div>
        </div>

        <div className="p-4 border rounded-md">
          <div className="font-medium mb-2">Primary Profile</div>
          <input
            type="text"
            value={profileName}
            onChange={(e) => onProfileNameChange(e.target.value)}
            placeholder="Profile name (e.g., prod)"
            className="w-full px-3 py-2 border rounded-md bg-background text-sm"
          />
        </div>

        {enableDr && (
          <div className="p-4 border rounded-md border-orange-200 dark:border-orange-800">
            <div className="font-medium mb-2 text-orange-700 dark:text-orange-300">
              DR Profile
            </div>
            <input
              type="text"
              value={drProfileName}
              onChange={(e) => onDrProfileNameChange(e.target.value)}
              placeholder="DR profile name (e.g., dr)"
              className="w-full px-3 py-2 border rounded-md bg-background text-sm mb-2"
            />
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={autoFailover}
                onChange={(e) => onAutoFailoverChange(e.target.checked)}
                className="rounded"
              />
              <span className="text-sm">Enable automatic failover</span>
            </label>
            <p className="text-xs text-muted-foreground mt-1">
              Automatically activate DR profile if primary agents become unreachable.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function isAllResolved(preview: ImportPreviewResponse, manualMappings: Record<string, string>): boolean {
  for (const comp of preview.components) {
    if (comp.resolution.status === 'resolved') continue;
    if (!manualMappings[comp.name]) return false;
  }
  return true;
}

function buildMappings(preview: ImportPreviewResponse, manualMappings: Record<string, string>): MappingConfig[] {
  return (preview.components || []).map((comp) => {
    if (comp.resolution.status === 'resolved') {
      return {
        component_name: comp.name,
        agent_id: comp.resolution.agent_id,
        resolved_via: comp.resolution.resolved_via,
      };
    }
    return {
      component_name: comp.name,
      agent_id: manualMappings[comp.name],
      resolved_via: 'manual',
    };
  });
}

function buildDrMappings(
  drSuggestions: Array<{ component_name: string; dr_resolution: ComponentResolutionStatus | null }>,
  manualMappings: Record<string, string>
): MappingConfig[] {
  return (drSuggestions || []).map((sug) => {
    const drKey = `dr_${sug.component_name}`;
    if (sug.dr_resolution?.status === 'resolved') {
      return {
        component_name: sug.component_name,
        agent_id: sug.dr_resolution.agent_id,
        resolved_via: 'pattern',
      };
    }
    return {
      component_name: sug.component_name,
      agent_id: manualMappings[drKey] || '',
      resolved_via: 'manual',
    };
  });
}
