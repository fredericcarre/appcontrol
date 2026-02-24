import { useState, useCallback } from 'react';
import { useParams } from 'react-router-dom';
import { useApp, useStartApp, useStopApp, useStartBranch } from '@/api/apps';
import { useStartComponent, useStopComponent, useForceStopComponent, useStartWithDeps } from '@/api/components';
import { usePermission } from '@/hooks/use-permission';
import { useWebSocket } from '@/hooks/use-websocket';
import { AppMap } from '@/components/maps/AppMap';
import { DetailPanel } from '@/components/maps/DetailPanel';
import { ShareModal } from '@/components/share/ShareModal';
import { CommandModal } from '@/components/commands/CommandModal';

export function MapViewPage() {
  const { appId } = useParams<{ appId: string }>();
  const { data: app, isLoading } = useApp(appId || '');
  const { canOperate } = usePermission(appId || '');
  const startApp = useStartApp();
  const stopApp = useStopApp();
  const startBranch = useStartBranch();
  const startComponent = useStartComponent();
  const stopComponent = useStopComponent();
  const forceStopComponent = useForceStopComponent();
  const startWithDeps = useStartWithDeps();
  const { subscribe } = useWebSocket();

  const [selectedComponentId, setSelectedComponentId] = useState<string | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandComponentId, setCommandComponentId] = useState<string | null>(null);

  // Subscribe to app events
  if (appId) {
    subscribe(appId);
  }

  const selectedComponent = app?.components.find((c) => c.id === selectedComponentId) || null;

  const handleStartAll = useCallback(() => {
    if (appId) startApp.mutate(appId);
  }, [appId, startApp]);

  const handleStopAll = useCallback(() => {
    if (appId) stopApp.mutate(appId);
  }, [appId, stopApp]);

  const handleRestartErrorBranch = useCallback(() => {
    if (appId) startBranch.mutate({ appId });
  }, [appId, startBranch]);

  const handleStartComponent = useCallback((id: string) => {
    startComponent.mutate(id);
  }, [startComponent]);

  const handleStopComponent = useCallback((id: string) => {
    stopComponent.mutate(id);
  }, [stopComponent]);

  const handleForceStopComponent = useCallback((id: string) => {
    if (window.confirm('Force kill this component? This will ignore all dependencies.')) {
      forceStopComponent.mutate(id);
    }
  }, [forceStopComponent]);

  const handleStartWithDeps = useCallback((id: string) => {
    startWithDeps.mutate(id);
  }, [startWithDeps]);

  const handleCommand = useCallback((componentId: string) => {
    setCommandComponentId(componentId);
    setCommandOpen(true);
  }, []);

  if (isLoading || !app) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="h-full flex">
      <div className="flex-1 relative">
        <div className="absolute top-0 left-0 right-0 p-4 z-10 pointer-events-none">
          <h1 className="text-xl font-bold pointer-events-auto inline-block bg-card/80 backdrop-blur px-3 py-1 rounded-md border border-border">
            {app.name}
          </h1>
        </div>
        <AppMap
          components={app.components || []}
          dependencies={app.dependencies || []}
          onSelectComponent={setSelectedComponentId}
          onStartAll={handleStartAll}
          onStopAll={handleStopAll}
          onRestartErrorBranch={handleRestartErrorBranch}
          onShare={() => setShareOpen(true)}
          onStartComponent={handleStartComponent}
          onStopComponent={handleStopComponent}
          onRestartComponent={handleStartComponent}
          onDiagnoseComponent={(id) => handleCommand(id)}
          onForceStopComponent={handleForceStopComponent}
          onStartWithDepsComponent={handleStartWithDeps}
          canOperate={canOperate}
        />
      </div>

      {selectedComponent && (
        <DetailPanel
          component={selectedComponent}
          onClose={() => setSelectedComponentId(null)}
          onStart={() => handleStartComponent(selectedComponent.id)}
          onStop={() => handleStopComponent(selectedComponent.id)}
          onRestart={() => handleStartComponent(selectedComponent.id)}
          onCommand={() => handleCommand(selectedComponent.id)}
          onDiagnose={() => handleCommand(selectedComponent.id)}
          onForceStop={() => handleForceStopComponent(selectedComponent.id)}
          onStartWithDeps={() => handleStartWithDeps(selectedComponent.id)}
          canOperate={canOperate}
        />
      )}

      <ShareModal
        appId={appId || ''}
        open={shareOpen}
        onOpenChange={setShareOpen}
      />

      {commandComponentId && (
        <CommandModal
          componentId={commandComponentId}
          open={commandOpen}
          onOpenChange={(open) => {
            setCommandOpen(open);
            if (!open) setCommandComponentId(null);
          }}
        />
      )}
    </div>
  );
}
