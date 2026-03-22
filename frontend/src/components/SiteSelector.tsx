/**
 * SiteSelector - Reusable component for selecting sites (not gateways)
 * Used by both OnboardingPage and ImportWizard
 */

import { Check, MapPin, Radio, AlertCircle } from 'lucide-react';
import { SiteSummary } from '@/api/gateways';
import { cn } from '@/lib/utils';

export interface SiteSelectorProps {
  sites: SiteSummary[];
  selectedSiteId: string | null;
  onSelect: (siteId: string | null) => void;
  label?: string;
  description?: string;
  emptyMessage?: string;
  /** Filter to only show sites with connected gateways */
  requireConnected?: boolean;
  /** Highlight style for DR sites */
  variant?: 'primary' | 'dr';
  /** Disable selection */
  disabled?: boolean;
}

export function SiteSelector({
  sites,
  selectedSiteId,
  onSelect,
  label = 'Select Site',
  description,
  emptyMessage = 'No sites available.',
  requireConnected = true,
  variant = 'primary',
  disabled = false,
}: SiteSelectorProps) {
  // Filter sites based on requirements
  const availableSites = sites.filter((site) => {
    if (!site.site_id) return false; // Skip unassigned gateways
    if (requireConnected) {
      return site.gateways.some((gw) => gw.connected);
    }
    return true;
  });

  const variantStyles = {
    primary: {
      selected: 'border-primary bg-primary/5',
      check: 'text-primary',
      icon: 'text-blue-600',
    },
    dr: {
      selected: 'border-orange-500 bg-orange-500/5',
      check: 'text-orange-500',
      icon: 'text-orange-600',
    },
  };

  const styles = variantStyles[variant];

  if (availableSites.length === 0) {
    return (
      <div className="p-4 border border-dashed border-border rounded-md text-center">
        <AlertCircle className="h-8 w-8 mx-auto text-muted-foreground mb-2" />
        <p className="text-sm text-muted-foreground">{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {label && <h3 className="text-lg font-medium">{label}</h3>}
      {description && (
        <p className="text-muted-foreground text-sm">{description}</p>
      )}
      <div className="grid gap-2">
        {availableSites.map((site) => {
          const isSelected = site.site_id === selectedSiteId;
          const connectedGateways = site.gateways.filter((gw) => gw.connected);
          const totalAgents = site.gateways.reduce((sum, gw) => sum + gw.agent_count, 0);

          return (
            <div
              key={site.site_id}
              onClick={() => !disabled && onSelect(isSelected ? null : site.site_id)}
              className={cn(
                'flex items-center justify-between p-4 border rounded-md transition-colors',
                disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer hover:bg-accent',
                isSelected ? styles.selected : 'border-border'
              )}
            >
              <div className="flex items-center gap-3">
                <div
                  className={cn(
                    'h-10 w-10 rounded-full flex items-center justify-center',
                    isSelected ? 'bg-primary text-primary-foreground' : 'bg-muted'
                  )}
                >
                  <MapPin className="h-5 w-5" />
                </div>
                <div>
                  <div className="font-medium flex items-center gap-2">
                    {site.site_name}
                    <span className="text-xs font-mono text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
                      {site.site_code}
                    </span>
                  </div>
                  <div className="text-sm text-muted-foreground flex items-center gap-3">
                    <span className="flex items-center gap-1">
                      <Radio className="h-3 w-3" />
                      {connectedGateways.length}/{site.gateways.length} gateway{site.gateways.length !== 1 ? 's' : ''} connected
                    </span>
                    <span>{totalAgents} agent{totalAgents !== 1 ? 's' : ''}</span>
                  </div>
                </div>
              </div>
              {isSelected && <Check className={cn('h-5 w-5', styles.check)} />}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/**
 * Get all gateway IDs for a site
 */
export function getGatewayIdsForSite(sites: SiteSummary[], siteId: string | null): string[] {
  if (!siteId) return [];
  const site = sites.find((s) => s.site_id === siteId);
  if (!site) return [];
  return site.gateways.map((gw) => gw.id);
}

/**
 * Get site info by ID
 */
export function getSiteById(sites: SiteSummary[], siteId: string | null): SiteSummary | null {
  if (!siteId) return null;
  return sites.find((s) => s.site_id === siteId) || null;
}
