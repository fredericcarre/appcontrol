import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import {
  Upload, MapPin, CheckCircle2, AlertTriangle, ArrowLeft, ArrowRight, Loader2,
  FileJson, FileCode, Shield, Check, HelpCircle, ChevronDown, ChevronRight, Terminal, Plus, Trash2
} from 'lucide-react';
import {
  useImportPreview,
  useImportExecute,
  ImportPreviewResponse,
  MappingConfig,
  ComponentResolution,
  AvailableAgent,
  ConflictAction,
} from '@/api/import';
import { useGatewaySites, SiteSummary } from '@/api/gateways';
import { JsonEditor, JsonError } from '@/components/JsonEditor';
import { cn } from '@/lib/utils';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

type WizardStep = 'upload' | 'sites' | 'resolution' | 'confirm';

// Per-component, per-site configuration
interface ComponentSiteConfig {
  enabled: boolean;  // Whether component is available on this site (default true)
  agentId: string;
  commandOverrides?: {
    check_cmd?: string;
    start_cmd?: string;
    stop_cmd?: string;
  };
}

// Site selection with type
interface SelectedSite {
  siteId: string;
  siteType: 'primary' | 'dr';
}

interface WizardState {
  // Step 1: Upload
  content: string;
  format: 'json' | 'yaml';
  jsonError: JsonError | null;
  // Step 2: Sites
  selectedSites: SelectedSite[];
  // Step 3: Resolution
  preview: ImportPreviewResponse | null;
  // componentName -> siteId -> config
  componentSiteConfigs: Record<string, Record<string, ComponentSiteConfig>>;
  // Step 4: Confirm
  autoFailover: boolean;
  // Conflict handling
  conflictAction: ConflictAction;
  newName: string;
}

const initialState: WizardState = {
  content: '',
  format: 'json',
  jsonError: null,
  selectedSites: [],
  preview: null,
  componentSiteConfigs: {},
  autoFailover: false,
  conflictAction: 'fail',
  newName: '',
};

// ═══════════════════════════════════════════════════════════════════════════
// Pre-select sites from binding_profiles in the imported JSON
// ═══════════════════════════════════════════════════════════════════════════

interface JsonBindingProfile {
  name?: string;
  profile_type?: string;
  site?: { code?: string; name?: string };
}

function extractSitesFromBindingProfiles(
  content: string,
  format: string,
  availableSites: SiteSummary[],
): SelectedSite[] {
  if (format !== 'json' || !content) return [];
  try {
    const parsed = JSON.parse(content);
    const app = parsed.application ?? parsed;
    const profiles: JsonBindingProfile[] = app?.binding_profiles;
    if (!Array.isArray(profiles) || profiles.length === 0) return [];

    const selected: SelectedSite[] = [];
    const usedSiteIds = new Set<string>();

    for (const profile of profiles) {
      const code = profile.site?.code;
      const name = profile.site?.name;
      if (!code && !name) continue;

      const match = availableSites.find(
        (s) => s.site_id && ((code && s.site_code === code) || (name && s.site_name === name)),
      );
      if (!match?.site_id || usedSiteIds.has(match.site_id)) continue;

      usedSiteIds.add(match.site_id);
      selected.push({
        siteId: match.site_id,
        siteType: profile.profile_type === 'dr' ? 'dr' : 'primary',
      });
    }

    // Ensure at most one primary
    const primaries = selected.filter((s) => s.siteType === 'primary');
    if (primaries.length > 1) {
      for (let i = 1; i < primaries.length; i++) {
        primaries[i].siteType = 'dr';
      }
    }

    return selected;
  } catch {
    return [];
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Main Wizard Component
// ═══════════════════════════════════════════════════════════════════════════

export default function ImportWizard() {
  const navigate = useNavigate();
  const [step, setStep] = useState<WizardStep>('upload');
  const [state, setState] = useState<WizardState>(initialState);

  const { data: sites = [] } = useGatewaySites();
  const previewMutation = useImportPreview();
  const executeMutation = useImportExecute();

  const updateState = useCallback((updates: Partial<WizardState>) => {
    setState((prev) => ({ ...prev, ...updates }));
  }, []);

  const steps: { key: WizardStep; label: string; icon: React.ReactNode }[] = [
    { key: 'upload', label: 'Upload', icon: <Upload className="h-4 w-4" /> },
    { key: 'sites', label: 'Sites', icon: <MapPin className="h-4 w-4" /> },
    { key: 'resolution', label: 'Resolution', icon: <CheckCircle2 className="h-4 w-4" /> },
    { key: 'confirm', label: 'Confirm', icon: <Shield className="h-4 w-4" /> },
  ];

  const currentStepIndex = steps.findIndex((s) => s.key === step);
  const progress = ((currentStepIndex + 1) / steps.length) * 100;

  // Get primary site
  const primarySite = state.selectedSites.find((s) => s.siteType === 'primary');
  const drSites = state.selectedSites.filter((s) => s.siteType === 'dr');

  // Get gateway IDs for all selected sites
  const getAllGatewayIds = (siteIds: string[]): string[] => {
    const ids: string[] = [];
    for (const siteId of siteIds) {
      const site = sites.find((s) => s.site_id === siteId);
      if (site) {
        ids.push(...site.gateways.map((g) => g.id));
      }
    }
    return ids;
  };

  // ─────────────────────────────────────────────────────────────────────────
  // Navigation handlers
  // ─────────────────────────────────────────────────────────────────────────

  const canProceed = (): boolean => {
    switch (step) {
      case 'upload':
        if (state.format === 'json' && state.jsonError) return false;
        return state.content.trim().length > 0;
      case 'sites':
        return !!primarySite;
      case 'resolution':
        return state.preview ? isAllResolved(state.preview, state.componentSiteConfigs, state.selectedSites) : false;
      case 'confirm': {
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
      // Pre-select sites from binding_profiles if present in the JSON
      if (state.selectedSites.length === 0) {
        const preSelected = extractSitesFromBindingProfiles(state.content, state.format, sites);
        if (preSelected.length > 0) {
          updateState({ selectedSites: preSelected });
        }
      }
      setStep('sites');
    } else if (step === 'sites') {
      // Preview with all selected site gateways
      const primaryGatewayIds = primarySite ? getAllGatewayIds([primarySite.siteId]) : [];
      const drGatewayIds = drSites.length > 0 ? getAllGatewayIds(drSites.map((s) => s.siteId)) : undefined;

      previewMutation.mutate(
        {
          content: state.content,
          format: state.format,
          gateway_ids: primaryGatewayIds,
          dr_gateway_ids: drGatewayIds,
        },
        {
          onSuccess: (data) => {
            // Auto-configure agents when only one is available per site
            const autoConfigs: Record<string, Record<string, ComponentSiteConfig>> = {};

            for (const comp of data.components || []) {
              autoConfigs[comp.name] = {};

              // Primary site: use resolved agent or auto-select if only one
              if (primarySite) {
                if (comp.resolution.status === 'resolved') {
                  autoConfigs[comp.name][primarySite.siteId] = {
                    enabled: true,
                    agentId: comp.resolution.agent_id,
                  };
                } else if (data.available_agents?.length === 1) {
                  autoConfigs[comp.name][primarySite.siteId] = {
                    enabled: true,
                    agentId: data.available_agents[0].agent_id,
                  };
                }
              }

              // DR sites: use dr_suggestions or auto-select
              if (data.dr_suggestions && data.dr_available_agents) {
                const suggestion = data.dr_suggestions.find((s) => s.component_name === comp.name);
                for (const drSite of drSites) {
                  if (suggestion?.dr_resolution?.status === 'resolved') {
                    autoConfigs[comp.name][drSite.siteId] = {
                      enabled: true,
                      agentId: suggestion.dr_resolution.agent_id,
                    };
                  } else if (data.dr_available_agents.length === 1) {
                    autoConfigs[comp.name][drSite.siteId] = {
                      enabled: true,
                      agentId: data.dr_available_agents[0].agent_id,
                    };
                  }
                }
              }
            }

            updateState({
              preview: data,
              componentSiteConfigs: { ...state.componentSiteConfigs, ...autoConfigs },
            });
            setStep('resolution');
          },
        }
      );
    } else if (step === 'resolution') {
      setStep('confirm');
    } else if (step === 'confirm') {
      // Build the request with profiles for each site
      const primarySiteInfo = sites.find((s) => s.site_id === primarySite?.siteId);

      // Build primary profile mappings
      const primaryMappings: MappingConfig[] = [];
      for (const comp of state.preview?.components || []) {
        const config = state.componentSiteConfigs[comp.name]?.[primarySite?.siteId || ''];
        if (config?.agentId) {
          primaryMappings.push({
            component_name: comp.name,
            agent_id: config.agentId,
            resolved_via: 'wizard',
          });
        }
      }

      // Inject site overrides into JSON content
      const contentWithOverrides = injectSiteOverrides(
        state.content,
        state.format,
        state.selectedSites,
        state.componentSiteConfigs,
        sites
      );

      // Build DR profile if DR sites exist
      let drProfile = undefined;
      if (drSites.length > 0) {
        const firstDrSite = sites.find((s) => s.site_id === drSites[0].siteId);
        const drGatewayIds = getAllGatewayIds(drSites.map((s) => s.siteId));

        const drMappings: MappingConfig[] = [];
        for (const comp of state.preview?.components || []) {
          const config = state.componentSiteConfigs[comp.name]?.[drSites[0].siteId];
          // Skip disabled components
          if (config?.enabled === false) continue;
          if (config?.agentId) {
            drMappings.push({
              component_name: comp.name,
              agent_id: config.agentId,
              resolved_via: 'wizard',
            });
          }
        }

        drProfile = {
          name: firstDrSite?.site_code?.toLowerCase() || 'dr',
          description: `DR configuration for ${firstDrSite?.site_name || 'DR site'}`,
          profile_type: 'dr' as const,
          gateway_ids: drGatewayIds,
          auto_failover: state.autoFailover,
          mappings: drMappings,
        };
      }

      executeMutation.mutate(
        {
          content: contentWithOverrides,
          format: state.format,
          site_id: primarySite?.siteId,
          profile: {
            name: primarySiteInfo?.site_code?.toLowerCase() || 'primary',
            description: `Primary configuration for ${primarySiteInfo?.site_name || 'default site'}`,
            profile_type: 'primary',
            gateway_ids: primarySite ? getAllGatewayIds([primarySite.siteId]) : [],
            mappings: primaryMappings,
          },
          dr_profile: drProfile,
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
    <div className="space-y-6">
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
          {step === 'sites' && (
            <SitesStep
              sites={sites}
              selectedSites={state.selectedSites}
              onSitesChange={(selectedSites) => updateState({ selectedSites })}
            />
          )}
          {step === 'resolution' && state.preview && (
            <ResolutionStep
              preview={state.preview}
              selectedSites={state.selectedSites}
              sites={sites}
              componentSiteConfigs={state.componentSiteConfigs}
              onConfigChange={(compName, siteId, config) => {
                updateState({
                  componentSiteConfigs: {
                    ...state.componentSiteConfigs,
                    [compName]: {
                      ...state.componentSiteConfigs[compName],
                      [siteId]: config,
                    },
                  },
                });
              }}
              content={state.content}
              format={state.format}
            />
          )}
          {step === 'confirm' && state.preview && (
            <ConfirmStep
              preview={state.preview}
              selectedSites={state.selectedSites}
              sites={sites}
              autoFailover={state.autoFailover}
              conflictAction={state.conflictAction}
              newName={state.newName}
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
                {(() => {
                  const err = previewMutation.error || executeMutation.error;
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  const axiosData = (err as any)?.response?.data;
                  return axiosData?.message || err?.message || 'An error occurred';
                })()}
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
          Supports JSON and YAML formats.
        </p>
      </div>

      <div className="flex gap-4">
        <Button
          variant={format === 'json' ? 'default' : 'outline'}
          onClick={() => onFormatChange('json')}
          className="flex-1"
        >
          <FileJson className="h-4 w-4 mr-2" />
          JSON
        </Button>
        <Button
          variant={format === 'yaml' ? 'default' : 'outline'}
          onClick={() => onFormatChange('yaml')}
          className="flex-1"
        >
          <FileCode className="h-4 w-4 mr-2" />
          YAML
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
            placeholder={'{\n  "name": "My App",\n  "components": []\n}'}
            height="350px"
          />
        ) : (
          <textarea
            value={content}
            onChange={(e) => onContentChange(e.target.value)}
            placeholder={'name: My App\ncomponents:\n  - name: component1\n    ...'}
            className="w-full h-64 px-3 py-2 border rounded-md bg-background text-sm font-mono"
          />
        )}
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 2: Multi-Site Selection
// ═══════════════════════════════════════════════════════════════════════════

interface SitesStepProps {
  sites: SiteSummary[];
  selectedSites: SelectedSite[];
  onSitesChange: (sites: SelectedSite[]) => void;
}

function SitesStep({ sites, selectedSites, onSitesChange }: SitesStepProps) {
  const availableSites = sites.filter((s) => s.site_id && s.gateways.some((g) => g.connected));
  const primarySite = selectedSites.find((s) => s.siteType === 'primary');
  const drSites = selectedSites.filter((s) => s.siteType === 'dr');

  const handlePrimarySelect = (siteId: string) => {
    const newSites = selectedSites.filter((s) => s.siteType !== 'primary' && s.siteId !== siteId);
    newSites.unshift({ siteId, siteType: 'primary' });
    onSitesChange(newSites);
  };

  const handleAddDrSite = (siteId: string) => {
    if (!selectedSites.find((s) => s.siteId === siteId)) {
      onSitesChange([...selectedSites, { siteId, siteType: 'dr' }]);
    }
  };

  const handleRemoveDrSite = (siteId: string) => {
    onSitesChange(selectedSites.filter((s) => s.siteId !== siteId));
  };

  const getSiteInfo = (siteId: string) => sites.find((s) => s.site_id === siteId);

  // Sites available for DR (not selected as primary or already DR)
  const availableDrSites = availableSites.filter(
    (s) => !selectedSites.find((sel) => sel.siteId === s.site_id)
  );

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Configure Sites</h3>
        <p className="text-muted-foreground text-sm">
          Select where your application will run. You can configure multiple DR sites.
        </p>
      </div>

      {/* Primary Site Selection */}
      <div className="space-y-3">
        <h4 className="font-medium flex items-center gap-2">
          <div className="w-3 h-3 rounded-full bg-emerald-500" />
          Primary Site
        </h4>
        <div className="grid gap-2">
          {availableSites.map((site) => {
            const isSelected = primarySite?.siteId === site.site_id;
            return (
              <div
                key={site.site_id}
                onClick={() => handlePrimarySelect(site.site_id!)}
                className={cn(
                  'flex items-center justify-between p-3 border rounded-md cursor-pointer transition-colors',
                  isSelected ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950' : 'hover:bg-accent'
                )}
              >
                <div className="flex items-center gap-3">
                  <MapPin className="h-4 w-4 text-muted-foreground" />
                  <div>
                    <span className="font-medium">{site.site_name}</span>
                    <span className="text-xs text-muted-foreground ml-2 font-mono">{site.site_code}</span>
                  </div>
                </div>
                {isSelected && <Check className="h-4 w-4 text-emerald-600" />}
              </div>
            );
          })}
        </div>
      </div>

      {/* DR Sites */}
      {primarySite && (
        <div className="space-y-3 pt-4 border-t">
          <h4 className="font-medium flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-orange-500" />
            DR Sites (Optional)
          </h4>

          {/* Selected DR sites */}
          {drSites.length > 0 && (
            <div className="space-y-2">
              {drSites.map((dr) => {
                const site = getSiteInfo(dr.siteId);
                return (
                  <div
                    key={dr.siteId}
                    className="flex items-center justify-between p-3 border border-orange-300 bg-orange-50 dark:bg-orange-950 rounded-md"
                  >
                    <div className="flex items-center gap-3">
                      <Shield className="h-4 w-4 text-orange-600" />
                      <div>
                        <span className="font-medium">{site?.site_name}</span>
                        <span className="text-xs text-muted-foreground ml-2 font-mono">{site?.site_code}</span>
                      </div>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleRemoveDrSite(dr.siteId)}
                    >
                      <Trash2 className="h-4 w-4 text-red-500" />
                    </Button>
                  </div>
                );
              })}
            </div>
          )}

          {/* Add DR site */}
          {availableDrSites.length > 0 && (
            <div className="flex items-center gap-2">
              <select
                className="flex-1 px-3 py-2 border rounded-md bg-background text-sm"
                value=""
                onChange={(e) => e.target.value && handleAddDrSite(e.target.value)}
              >
                <option value="">Add DR site...</option>
                {availableDrSites.map((site) => (
                  <option key={site.site_id} value={site.site_id!}>
                    {site.site_name} ({site.site_code})
                  </option>
                ))}
              </select>
              <Plus className="h-4 w-4 text-muted-foreground" />
            </div>
          )}

          {drSites.length === 0 && availableDrSites.length === 0 && (
            <p className="text-sm text-muted-foreground">No other sites available for DR.</p>
          )}
        </div>
      )}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 3: Resolution (Multi-Site)
// ═══════════════════════════════════════════════════════════════════════════

// Parse original component commands from import content
interface ParsedComponentCommands {
  check_cmd?: string;
  start_cmd?: string;
  stop_cmd?: string;
}

function parseComponentCommands(content: string, format: string): Record<string, ParsedComponentCommands> {
  const result: Record<string, ParsedComponentCommands> = {};
  if (format !== 'json') return result;
  try {
    const data = JSON.parse(content);
    const app = data.application || data;
    for (const comp of app.components || []) {
      result[comp.name] = {
        check_cmd: comp.check_cmd || '',
        start_cmd: comp.start_cmd || '',
        stop_cmd: comp.stop_cmd || '',
      };
    }
  } catch { /* ignore parse errors */ }
  return result;
}

interface ResolutionStepProps {
  preview: ImportPreviewResponse;
  selectedSites: SelectedSite[];
  sites: SiteSummary[];
  componentSiteConfigs: Record<string, Record<string, ComponentSiteConfig>>;
  onConfigChange: (compName: string, siteId: string, config: ComponentSiteConfig) => void;
  content: string;
  format: string;
}

function ResolutionStep({
  preview,
  selectedSites,
  sites,
  componentSiteConfigs,
  onConfigChange,
  content,
  format,
}: ResolutionStepProps) {
  const primarySite = selectedSites.find((s) => s.siteType === 'primary');
  const originalCommands = parseComponentCommands(content, format);

  const getSiteInfo = (siteId: string) => sites.find((s) => s.site_id === siteId);

  // Get agents for a site
  const getAgentsForSite = (siteId: string): AvailableAgent[] => {
    const site = getSiteInfo(siteId);
    if (!site) return [];

    // For primary site, use available_agents
    if (siteId === primarySite?.siteId) {
      return preview.available_agents || [];
    }
    // For DR sites, use dr_available_agents
    return preview.dr_available_agents || [];
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-medium mb-2">Configure Components per Site</h3>
        <p className="text-muted-foreground text-sm">
          {preview.application_name} - {preview.component_count} components across {selectedSites.length} site(s)
        </p>
      </div>

      {/* Components */}
      <div className="space-y-4">
        {(preview.components || []).map((comp) => (
          <ComponentSiteRow
            key={comp.name}
            component={comp}
            selectedSites={selectedSites}
            sites={sites}
            configs={componentSiteConfigs[comp.name] || {}}
            getAgentsForSite={getAgentsForSite}
            onConfigChange={(siteId, config) => onConfigChange(comp.name, siteId, config)}
            originalCommands={originalCommands[comp.name]}
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

interface ComponentSiteRowProps {
  component: ComponentResolution;
  selectedSites: SelectedSite[];
  sites: SiteSummary[];
  configs: Record<string, ComponentSiteConfig>;
  getAgentsForSite: (siteId: string) => AvailableAgent[];
  onConfigChange: (siteId: string, config: ComponentSiteConfig) => void;
  originalCommands?: ParsedComponentCommands;
}

function ComponentSiteRow({
  component,
  selectedSites,
  sites,
  configs,
  getAgentsForSite,
  onConfigChange,
  originalCommands,
}: ComponentSiteRowProps) {
  const [expanded, setExpanded] = useState(false);

  const getSiteInfo = (siteId: string) => sites.find((s) => s.site_id === siteId);

  // Check if all sites are configured (respecting enabled status)
  const allConfigured = selectedSites.every((s) => {
    const cfg = configs[s.siteId];
    // If explicitly disabled, consider it configured
    if (cfg?.enabled === false) return true;
    // Otherwise, must have an agent
    return cfg?.agentId;
  });

  const hasOverrides = selectedSites.some((s) => {
    const cfg = configs[s.siteId];
    if (cfg?.enabled === false) return false;
    return cfg?.commandOverrides?.check_cmd || cfg?.commandOverrides?.start_cmd || cfg?.commandOverrides?.stop_cmd;
  });

  // Count disabled DR sites
  const disabledDrCount = selectedSites.filter((s) =>
    s.siteType === 'dr' && configs[s.siteId]?.enabled === false
  ).length;

  return (
    <div className="border rounded-md">
      {/* Header */}
      <div
        className="flex items-center gap-3 p-3 cursor-pointer hover:bg-accent/50"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            {allConfigured ? (
              <Check className="h-4 w-4 text-green-600" />
            ) : (
              <HelpCircle className="h-4 w-4 text-amber-600" />
            )}
            <span className="font-medium">{component.name}</span>
            <span className="text-xs text-muted-foreground">({component.component_type})</span>
            {hasOverrides && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-300">
                <Terminal className="h-3 w-3 inline mr-0.5" />
                custom
              </span>
            )}
            {disabledDrCount > 0 && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
                {disabledDrCount} DR skipped
              </span>
            )}
          </div>
          <div className="text-xs text-muted-foreground mt-0.5">
            {component.host || 'no host'} → {selectedSites.length - disabledDrCount} site(s)
          </div>
        </div>
      </div>

      {/* Expanded content */}
      {expanded && (
        <div className="border-t p-3 space-y-4">
          {selectedSites.map((selectedSite) => {
            const siteInfo = getSiteInfo(selectedSite.siteId);
            const agents = getAgentsForSite(selectedSite.siteId);
            const config = configs[selectedSite.siteId] || {};
            const isPrimary = selectedSite.siteType === 'primary';
            const isEnabled = config.enabled !== false; // Default to true

            return (
              <div
                key={selectedSite.siteId}
                className={cn(
                  'p-3 rounded-md border',
                  isPrimary
                    ? 'border-emerald-200 bg-emerald-50/50 dark:bg-emerald-950/30'
                    : isEnabled
                      ? 'border-orange-200 bg-orange-50/50 dark:bg-orange-950/30'
                      : 'border-border bg-muted/30 opacity-60'
                )}
              >
                <div className="flex items-center justify-between mb-3">
                  <div className="flex items-center gap-2">
                    <div className={cn(
                      'w-2 h-2 rounded-full',
                      isPrimary ? 'bg-emerald-500' : isEnabled ? 'bg-orange-500' : 'bg-muted-foreground'
                    )} />
                    <span className="font-medium text-sm">{siteInfo?.site_name}</span>
                    <span className="text-xs text-muted-foreground font-mono">{siteInfo?.site_code}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">{isPrimary ? 'Primary' : 'DR'}</span>
                    {/* Enable/Disable toggle for DR sites */}
                    {!isPrimary && (
                      <label className="flex items-center gap-1.5 cursor-pointer">
                        <span className="text-xs text-muted-foreground">
                          {isEnabled ? 'Enabled' : 'Disabled'}
                        </span>
                        <input
                          type="checkbox"
                          checked={isEnabled}
                          onChange={(e) => {
                            e.stopPropagation();
                            onConfigChange(selectedSite.siteId, { ...config, enabled: e.target.checked });
                          }}
                          className="h-3.5 w-3.5 rounded"
                        />
                      </label>
                    )}
                  </div>
                </div>

                {/* Show content only if enabled (or primary) */}
                {(isPrimary || isEnabled) ? (
                  <>
                    {/* Agent selection */}
                    <div className="space-y-2">
                      <label className="text-xs text-muted-foreground">Agent</label>
                      <select
                        value={config.agentId || ''}
                        onChange={(e) => onConfigChange(selectedSite.siteId, { ...config, agentId: e.target.value })}
                        className="w-full px-2 py-1.5 border rounded bg-background text-sm"
                      >
                        <option value="">Select agent...</option>
                        {agents.map((a) => (
                          <option key={a.agent_id} value={a.agent_id}>
                            {a.hostname}
                          </option>
                        ))}
                      </select>
                    </div>

                    {/* Commands section */}
                    {isPrimary ? (
                      <div className="mt-3 pt-3 border-t border-dashed space-y-2">
                        <label className="text-xs text-muted-foreground flex items-center gap-1">
                          <Terminal className="h-3 w-3" />
                          Commands
                        </label>
                        <div className="space-y-1.5">
                          <div>
                            <span className="text-[10px] text-muted-foreground">check_cmd</span>
                            <input
                              type="text"
                              placeholder="Health check command"
                              value={config.commandOverrides?.check_cmd ?? originalCommands?.check_cmd ?? ''}
                              onChange={(e) => onConfigChange(selectedSite.siteId, {
                                ...config,
                                commandOverrides: { ...config.commandOverrides, check_cmd: e.target.value }
                              })}
                              className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                            />
                          </div>
                          <div>
                            <span className="text-[10px] text-muted-foreground">start_cmd</span>
                            <input
                              type="text"
                              placeholder="Start command"
                              value={config.commandOverrides?.start_cmd ?? originalCommands?.start_cmd ?? ''}
                              onChange={(e) => onConfigChange(selectedSite.siteId, {
                                ...config,
                                commandOverrides: { ...config.commandOverrides, start_cmd: e.target.value }
                              })}
                              className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                            />
                          </div>
                          <div>
                            <span className="text-[10px] text-muted-foreground">stop_cmd</span>
                            <input
                              type="text"
                              placeholder="Stop command"
                              value={config.commandOverrides?.stop_cmd ?? originalCommands?.stop_cmd ?? ''}
                              onChange={(e) => onConfigChange(selectedSite.siteId, {
                                ...config,
                                commandOverrides: { ...config.commandOverrides, stop_cmd: e.target.value }
                              })}
                              className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                            />
                          </div>
                        </div>
                      </div>
                    ) : (
                      <div className="mt-3 pt-3 border-t border-dashed space-y-2">
                        <label className="text-xs text-muted-foreground flex items-center gap-1">
                          <Terminal className="h-3 w-3" />
                          Command Overrides (optional)
                        </label>
                        <input
                          type="text"
                          placeholder="Check command override"
                          value={config.commandOverrides?.check_cmd || ''}
                          onChange={(e) => onConfigChange(selectedSite.siteId, {
                            ...config,
                            commandOverrides: { ...config.commandOverrides, check_cmd: e.target.value || undefined }
                          })}
                          className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                        />
                        <input
                          type="text"
                          placeholder="Start command override"
                          value={config.commandOverrides?.start_cmd || ''}
                          onChange={(e) => onConfigChange(selectedSite.siteId, {
                            ...config,
                            commandOverrides: { ...config.commandOverrides, start_cmd: e.target.value || undefined }
                          })}
                          className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                        />
                        <input
                          type="text"
                          placeholder="Stop command override"
                          value={config.commandOverrides?.stop_cmd || ''}
                          onChange={(e) => onConfigChange(selectedSite.siteId, {
                            ...config,
                            commandOverrides: { ...config.commandOverrides, stop_cmd: e.target.value || undefined }
                          })}
                          className="w-full px-2 py-1 border rounded bg-background text-xs font-mono"
                        />
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
      )}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Step 4: Confirm
// ═══════════════════════════════════════════════════════════════════════════

interface ConfirmStepProps {
  preview: ImportPreviewResponse;
  selectedSites: SelectedSite[];
  sites: SiteSummary[];
  autoFailover: boolean;
  conflictAction: ConflictAction;
  newName: string;
  onAutoFailoverChange: (enabled: boolean) => void;
  onConflictActionChange: (action: ConflictAction) => void;
  onNewNameChange: (name: string) => void;
}

function ConfirmStep({
  preview,
  selectedSites,
  sites,
  autoFailover,
  conflictAction,
  newName,
  onAutoFailoverChange,
  onConflictActionChange,
  onNewNameChange,
}: ConfirmStepProps) {
  const primarySite = selectedSites.find((s) => s.siteType === 'primary');
  const drSites = selectedSites.filter((s) => s.siteType === 'dr');

  const getSiteInfo = (siteId: string) => sites.find((s) => s.site_id === siteId);

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
          <div className="space-y-2">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                name="conflict"
                checked={conflictAction === 'rename'}
                onChange={() => onConflictActionChange('rename')}
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
              />
              <span className="text-sm">Update existing application</span>
            </label>
          </div>
        </div>
      )}

      {/* Summary */}
      <div className="grid gap-4">
        {/* Application */}
        <div className="p-4 border rounded-md">
          <div className="font-medium mb-2">Application</div>
          <div className="text-lg">
            {conflictAction === 'rename' && newName ? newName : preview.application_name}
          </div>
          <div className="text-sm text-muted-foreground">{preview.component_count} components</div>
        </div>

        {/* Sites summary */}
        <div className="p-4 border rounded-md">
          <div className="font-medium mb-3">Sites Configuration</div>
          <div className="space-y-2">
            {primarySite && (
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-emerald-500" />
                <span className="text-sm font-medium">{getSiteInfo(primarySite.siteId)?.site_name}</span>
                <span className="text-xs text-muted-foreground">(Primary)</span>
              </div>
            )}
            {drSites.map((dr) => (
              <div key={dr.siteId} className="flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-orange-500" />
                <span className="text-sm font-medium">{getSiteInfo(dr.siteId)?.site_name}</span>
                <span className="text-xs text-muted-foreground">(DR)</span>
              </div>
            ))}
          </div>
        </div>

        {/* Auto failover */}
        {drSites.length > 0 && (
          <div className="p-4 border border-orange-200 rounded-md bg-orange-50/50 dark:bg-orange-950/30">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={autoFailover}
                onChange={(e) => onAutoFailoverChange(e.target.checked)}
                className="rounded"
              />
              <span className="text-sm font-medium">Enable automatic failover</span>
            </label>
            <p className="text-xs text-muted-foreground mt-1 pl-6">
              Automatically switch to DR site if primary becomes unreachable.
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

function isAllResolved(
  preview: ImportPreviewResponse,
  configs: Record<string, Record<string, ComponentSiteConfig>>,
  selectedSites: SelectedSite[]
): boolean {
  for (const comp of preview.components) {
    for (const site of selectedSites) {
      const config = configs[comp.name]?.[site.siteId];
      // If explicitly disabled, no agent needed
      if (config?.enabled === false) continue;
      // If enabled (default), must have an agent
      if (!config?.agentId) {
        return false;
      }
    }
  }
  return true;
}

function injectSiteOverrides(
  content: string,
  format: string,
  selectedSites: SelectedSite[],
  configs: Record<string, Record<string, ComponentSiteConfig>>,
  sites: SiteSummary[]
): string {
  if (format !== 'json') return content;

  const primarySite = selectedSites.find((s) => s.siteType === 'primary');
  const drSites = selectedSites.filter((s) => s.siteType === 'dr');

  // Check if there's anything to inject
  const hasPrimaryEdits = primarySite && Object.values(configs).some((c) => {
    const cfg = c[primarySite.siteId];
    return cfg?.commandOverrides?.check_cmd !== undefined
      || cfg?.commandOverrides?.start_cmd !== undefined
      || cfg?.commandOverrides?.stop_cmd !== undefined;
  });
  if (drSites.length === 0 && !hasPrimaryEdits) return content;

  try {
    const data = JSON.parse(content);
    const app = data.application || data;
    const components = app.components || [];

    for (const comp of components) {
      const compName = comp.name;
      const compConfigs = configs[compName];
      if (!compConfigs) continue;

      // Apply primary site command edits directly to the component
      if (primarySite) {
        const primaryConfig = compConfigs[primarySite.siteId];
        if (primaryConfig?.commandOverrides) {
          if (primaryConfig.commandOverrides.check_cmd !== undefined) {
            comp.check_cmd = primaryConfig.commandOverrides.check_cmd;
          }
          if (primaryConfig.commandOverrides.start_cmd !== undefined) {
            comp.start_cmd = primaryConfig.commandOverrides.start_cmd;
          }
          if (primaryConfig.commandOverrides.stop_cmd !== undefined) {
            comp.stop_cmd = primaryConfig.commandOverrides.stop_cmd;
          }
        }
      }

      // Add override for each DR site that has command overrides
      if (drSites.length > 0) {
        if (!comp.site_overrides) {
          comp.site_overrides = [];
        }

        for (const drSite of drSites) {
          const siteConfig = compConfigs[drSite.siteId];
          const siteInfo = sites.find((s) => s.site_id === drSite.siteId);
          if (!siteInfo || !siteConfig) continue;

          // Skip disabled components
          if (siteConfig.enabled === false) continue;

          // Find or create override for this site
          let override = comp.site_overrides.find(
            (o: { site_code: string }) => o.site_code === siteInfo.site_code
          );
          if (!override) {
            override = { site_code: siteInfo.site_code };
            comp.site_overrides.push(override);
          }

          // Apply command overrides if present
          if (siteConfig.commandOverrides?.check_cmd) {
            override.check_cmd_override = siteConfig.commandOverrides.check_cmd;
          }
          if (siteConfig.commandOverrides?.start_cmd) {
            override.start_cmd_override = siteConfig.commandOverrides.start_cmd;
          }
          if (siteConfig.commandOverrides?.stop_cmd) {
            override.stop_cmd_override = siteConfig.commandOverrides.stop_cmd;
          }
        }
      }
    }

    return JSON.stringify(data, null, 2);
  } catch (e) {
    console.warn('Failed to inject site overrides:', e);
    return content;
  }
}
