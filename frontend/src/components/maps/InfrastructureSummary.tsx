import { useMemo } from 'react';
import { Server, Radio, WifiOff, Wifi, HelpCircle } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { Component } from '@/api/apps';

interface InfrastructureInfo {
  id: string;
  name: string;
  type: 'gateway' | 'agent';
  connected: boolean;
  componentCount: number;
  componentIds: string[];
}

interface InfrastructureSummaryProps {
  components: Component[];
  onHighlightComponents?: (componentIds: string[]) => void;
  onClearHighlight?: () => void;
}

export function InfrastructureSummary({
  components,
  onHighlightComponents,
  onClearHighlight,
}: InfrastructureSummaryProps) {
  // Extract unique gateways and agents from components
  const infrastructure = useMemo(() => {
    const gateways = new Map<string, InfrastructureInfo>();
    const agents = new Map<string, InfrastructureInfo>();
    let noAgentCount = 0;
    const noAgentComponentIds: string[] = [];

    for (const c of components) {
      // Track gateway
      if (c.gateway_id) {
        if (!gateways.has(c.gateway_id)) {
          gateways.set(c.gateway_id, {
            id: c.gateway_id,
            name: c.gateway_name || c.gateway_id.slice(0, 8),
            type: 'gateway',
            connected: c.gateway_connected ?? false,
            componentCount: 0,
            componentIds: [],
          });
        }
        const gw = gateways.get(c.gateway_id)!;
        gw.componentCount++;
        gw.componentIds.push(c.id);
        // Update gateway name if not already set and we have one
        if (c.gateway_name && gw.name === gw.id.slice(0, 8)) {
          gw.name = c.gateway_name;
        }
        // Update connected status - if any component shows connected, gateway is connected
        if (c.gateway_connected) gw.connected = true;
      }

      // Track agent
      if (c.agent_id) {
        if (!agents.has(c.agent_id)) {
          agents.set(c.agent_id, {
            id: c.agent_id,
            name: c.agent_hostname || 'Unknown Agent',
            type: 'agent',
            connected: c.agent_connected ?? false,
            componentCount: 0,
            componentIds: [],
          });
        }
        const agent = agents.get(c.agent_id)!;
        agent.componentCount++;
        agent.componentIds.push(c.id);
        // Update hostname if not set
        if (c.agent_hostname && agent.name === 'Unknown Agent') {
          agent.name = c.agent_hostname;
        }
        // Update connected status
        if (c.agent_connected) agent.connected = true;
      } else {
        noAgentCount++;
        noAgentComponentIds.push(c.id);
      }
    }

    return {
      gateways: Array.from(gateways.values()),
      agents: Array.from(agents.values()),
      noAgentCount,
      noAgentComponentIds,
    };
  }, [components]);

  const { gateways, agents, noAgentCount, noAgentComponentIds } = infrastructure;

  // Don't render if no infrastructure
  if (gateways.length === 0 && agents.length === 0 && noAgentCount === 0) {
    return null;
  }

  const handleMouseEnter = (componentIds: string[]) => {
    if (onHighlightComponents && componentIds.length > 0) {
      onHighlightComponents(componentIds);
    }
  };

  const handleMouseLeave = () => {
    if (onClearHighlight) {
      onClearHighlight();
    }
  };

  return (
    <TooltipProvider>
      {/* Position at bottom right to avoid overlapping with toolbar */}
      <div className="absolute bottom-4 right-48 z-10 flex flex-col gap-2">
        {/* Gateways */}
        {gateways.length > 0 && (
          <div className="bg-card/95 backdrop-blur border border-border rounded-md px-3 py-2 shadow-sm">
            <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wide mb-1.5">
              Gateways
            </div>
            <div className="flex flex-wrap gap-1.5">
              {gateways.map((gw) => (
                <Tooltip key={gw.id}>
                  <TooltipTrigger asChild>
                    <button
                      className={`
                        inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs
                        transition-colors hover:ring-1 hover:ring-primary/50
                        ${gw.connected
                          ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/30 dark:text-emerald-300'
                          : 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300'
                        }
                      `}
                      onMouseEnter={() => handleMouseEnter(gw.componentIds)}
                      onMouseLeave={handleMouseLeave}
                    >
                      <Radio className="h-3 w-3" />
                      <span className="max-w-[120px] truncate">{gw.name}</span>
                      {gw.connected ? (
                        <Wifi className="h-2.5 w-2.5" />
                      ) : (
                        <WifiOff className="h-2.5 w-2.5" />
                      )}
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="top">
                    <div className="text-xs">
                      <div className="font-medium">Gateway: {gw.name}</div>
                      <div className="text-muted-foreground">
                        {gw.componentCount} component{gw.componentCount !== 1 ? 's' : ''} •{' '}
                        {gw.connected ? 'Connected' : 'Disconnected'}
                      </div>
                    </div>
                  </TooltipContent>
                </Tooltip>
              ))}
            </div>
          </div>
        )}

        {/* Agents */}
        {agents.length > 0 && (
          <div className="bg-card/95 backdrop-blur border border-border rounded-md px-3 py-2 shadow-sm">
            <div className="text-[10px] font-medium text-muted-foreground uppercase tracking-wide mb-1.5">
              Agents ({agents.length})
            </div>
            <div className="flex flex-wrap gap-1.5 max-w-[300px]">
              {agents.map((agent) => (
                <Tooltip key={agent.id}>
                  <TooltipTrigger asChild>
                    <button
                      className={`
                        inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs
                        transition-colors hover:ring-1 hover:ring-primary/50
                        ${agent.connected
                          ? 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300'
                          : 'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300'
                        }
                      `}
                      onMouseEnter={() => handleMouseEnter(agent.componentIds)}
                      onMouseLeave={handleMouseLeave}
                    >
                      <Server className="h-3 w-3" />
                      <span className="max-w-[100px] truncate">{agent.name}</span>
                      {agent.connected ? (
                        <Wifi className="h-2.5 w-2.5" />
                      ) : (
                        <WifiOff className="h-2.5 w-2.5" />
                      )}
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="top">
                    <div className="text-xs">
                      <div className="font-medium">Agent: {agent.name}</div>
                      <div className="text-muted-foreground">
                        {agent.componentCount} component{agent.componentCount !== 1 ? 's' : ''} •{' '}
                        {agent.connected ? 'Connected' : 'Disconnected'}
                      </div>
                    </div>
                  </TooltipContent>
                </Tooltip>
              ))}
            </div>
          </div>
        )}

        {/* No agent warning */}
        {noAgentCount > 0 && (
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                className="inline-flex items-center gap-1.5 bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-300 px-3 py-1.5 rounded-md text-xs hover:ring-1 hover:ring-amber-500/50"
                onMouseEnter={() => handleMouseEnter(noAgentComponentIds)}
                onMouseLeave={handleMouseLeave}
              >
                <HelpCircle className="h-3.5 w-3.5" />
                {noAgentCount} component{noAgentCount !== 1 ? 's' : ''} without agent
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">
              <div className="text-xs max-w-[200px]">
                Components without an assigned agent cannot execute commands or be monitored.
              </div>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
    </TooltipProvider>
  );
}
