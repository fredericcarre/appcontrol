import { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import {
  useExecuteCommand,
  useCustomCommands,
  useCommandParams,
  useCommandExecutions,
  type CommandInputParam,
} from '@/api/components';
import { useWebSocketStore } from '@/stores/websocket';
import type { WsMessage } from '@/stores/websocket';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Play, Terminal, History, Loader2 } from 'lucide-react';

interface CommandModalProps {
  componentId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

interface OutputLine {
  text: string;
  timestamp: string;
  type: 'stdout' | 'stderr' | 'info';
}

export function CommandModal({ componentId, open, onOpenChange }: CommandModalProps) {
  const executeCommand = useExecuteCommand();
  const { data: customCommands } = useCustomCommands(componentId);
  const { data: executions } = useCommandExecutions(componentId);

  const [commandType, setCommandType] = useState('check');
  const [selectedCommandId, setSelectedCommandId] = useState<string | null>(null);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [output, setOutput] = useState<OutputLine[]>([]);
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Get params for the selected custom command
  const { data: commandParams } = useCommandParams(selectedCommandId);

  // When custom command selection changes, update selectedCommandId and reset params
  const handleCommandTypeChange = useCallback(
    (value: string) => {
      setCommandType(value);
      if (value.startsWith('custom:')) {
        const cmdName = value.replace('custom:', '');
        const cmd = customCommands?.find((c) => c.name === cmdName);
        setSelectedCommandId(cmd?.id ?? null);
      } else {
        setSelectedCommandId(null);
      }
      setParamValues({});
    },
    [customCommands],
  );

  // Compute default values from command params definition (stable reference)
  const paramDefaults = useMemo(() => {
    if (!commandParams) return {};
    const defaults: Record<string, string> = {};
    for (const p of commandParams) {
      if (p.default_value !== null) {
        defaults[p.name] = p.default_value;
      }
    }
    return defaults;
  }, [commandParams]);

  // Auto-scroll output
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [output]);

  // Process a single WebSocket message related to the active request
  const processWsMessage = useCallback(
    (msg: WsMessage) => {
      const payload = msg.payload as Record<string, unknown>;
      if (msg.type === 'CommandOutputChunkEvent') {
        const stdout = payload.stdout as string;
        const stderr = payload.stderr as string;
        const lines: OutputLine[] = [];
        if (stdout) lines.push({ text: stdout, timestamp: new Date().toISOString(), type: 'stdout' });
        if (stderr) lines.push({ text: stderr, timestamp: new Date().toISOString(), type: 'stderr' });
        if (lines.length > 0) setOutput((prev) => [...prev, ...lines]);
      } else if (msg.type === 'CommandResultEvent') {
        const exitCode = payload.exit_code as number;
        const stdout = payload.stdout as string;
        const stderr = payload.stderr as string;
        const lines: OutputLine[] = [];
        if (stdout) lines.push({ text: stdout, timestamp: new Date().toISOString(), type: 'stdout' });
        if (stderr) lines.push({ text: stderr, timestamp: new Date().toISOString(), type: 'stderr' });
        lines.push({
          text: `Exit code: ${exitCode}`,
          timestamp: new Date().toISOString(),
          type: exitCode === 0 ? 'info' : 'stderr',
        });
        setOutput((prev) => [...prev, ...lines]);
        setActiveRequestId(null);
      }
    },
    [],
  );

  // Subscribe to WebSocket store messages for streaming output
  const messages = useWebSocketStore((s) => s.messages);
  const processedCountRef = useRef(0);
  useEffect(() => {
    if (!activeRequestId) {
      processedCountRef.current = messages.length;
      return;
    }
    // Process only new messages since last check
    const newMessages = messages.slice(processedCountRef.current);
    processedCountRef.current = messages.length;

    for (const msg of newMessages) {
      const payload = msg.payload as Record<string, unknown>;
      if (
        (msg.type === 'CommandOutputChunkEvent' || msg.type === 'CommandResultEvent') &&
        payload.request_id === activeRequestId
      ) {
        processWsMessage(msg);
      }
    }
  }, [messages, activeRequestId, processWsMessage]);

  const handleExecute = async () => {
    const now = new Date().toISOString();
    const cmdLabel = commandType.startsWith('custom:')
      ? commandType.replace('custom:', '')
      : commandType;
    setOutput((prev) => [
      ...prev,
      { text: `> Executing ${cmdLabel}...`, timestamp: now, type: 'info' },
    ]);

    try {
      const result = await executeCommand.mutateAsync({
        component_id: componentId,
        command_type: cmdLabel,
        parameters: commandParams && commandParams.length > 0 ? { ...paramDefaults, ...paramValues } : undefined,
      });

      setActiveRequestId(result.request_id);
      setOutput((prev) => [
        ...prev,
        {
          text: `Command dispatched (request_id: ${result.request_id})`,
          timestamp: new Date().toISOString(),
          type: 'info',
        },
      ]);
    } catch (err: unknown) {
      let message = 'Command failed';
      // Check for axios error with response
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const axiosErr = err as any;
      if (axiosErr?.response?.status === 404) {
        if (cmdLabel.startsWith('custom:') || customCommands?.some((c) => c.name === cmdLabel)) {
          message = `Custom command "${cmdLabel}" not found. It may have been deleted.`;
        } else {
          message = `Command "${cmdLabel}" not found for this component.`;
        }
      } else if (axiosErr?.response?.data?.error) {
        message = axiosErr.response.data.error;
      } else if (err instanceof Error) {
        message = err.message;
      }
      setOutput((prev) => [
        ...prev,
        { text: message, timestamp: new Date().toISOString(), type: 'stderr' },
      ]);
    }
  };

  const renderParamField = (param: CommandInputParam) => {
    const value = paramValues[param.name] ?? paramDefaults[param.name] ?? '';

    switch (param.param_type) {
      case 'boolean':
        return (
          <Select value={value || 'false'} onValueChange={(v) => setParamValues((prev) => ({ ...prev, [param.name]: v }))}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="true">True</SelectItem>
              <SelectItem value="false">False</SelectItem>
            </SelectContent>
          </Select>
        );
      case 'enum':
        return (
          <Select value={value} onValueChange={(v) => setParamValues((prev) => ({ ...prev, [param.name]: v }))}>
            <SelectTrigger className="w-full">
              <SelectValue placeholder="Select..." />
            </SelectTrigger>
            <SelectContent>
              {(param.enum_values ?? []).map((ev) => (
                <SelectItem key={ev} value={ev}>
                  {ev}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        );
      case 'number':
        return (
          <Input
            type="number"
            value={value}
            onChange={(e) => setParamValues((prev) => ({ ...prev, [param.name]: e.target.value }))}
            placeholder={param.default_value ?? ''}
          />
        );
      case 'date':
        return (
          <Input
            type="date"
            value={value}
            onChange={(e) => setParamValues((prev) => ({ ...prev, [param.name]: e.target.value }))}
          />
        );
      case 'password':
        return (
          <Input
            type="password"
            value={value}
            onChange={(e) => setParamValues((prev) => ({ ...prev, [param.name]: e.target.value }))}
            placeholder={param.description ?? ''}
          />
        );
      default:
        return (
          <Input
            value={value}
            onChange={(e) => setParamValues((prev) => ({ ...prev, [param.name]: e.target.value }))}
            placeholder={param.default_value ?? param.description ?? ''}
          />
        );
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Terminal className="h-5 w-5" /> Execute Command
          </DialogTitle>
        </DialogHeader>

        <Tabs defaultValue="execute" className="flex-1 flex flex-col min-h-0">
          <TabsList>
            <TabsTrigger value="execute" className="gap-1">
              <Play className="h-3.5 w-3.5" /> Execute
            </TabsTrigger>
            <TabsTrigger value="history" className="gap-1">
              <History className="h-3.5 w-3.5" /> History
            </TabsTrigger>
          </TabsList>

          <TabsContent value="execute" className="flex-1 flex flex-col min-h-0 space-y-3">
            {/* Command type selector */}
            <div className="flex gap-2">
              <Select value={commandType} onValueChange={handleCommandTypeChange}>
                <SelectTrigger className="w-52">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="check">Health Check</SelectItem>
                  <SelectItem value="start">Start</SelectItem>
                  <SelectItem value="stop">Stop</SelectItem>
                  <SelectItem value="restart">Restart</SelectItem>
                  <SelectItem value="integrity_check">Integrity Check</SelectItem>
                  <SelectItem value="infra_check">Infra Check</SelectItem>
                  {customCommands?.map((cmd) => (
                    <SelectItem key={cmd.id} value={`custom:${cmd.name}`}>
                      {cmd.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Button
                onClick={handleExecute}
                disabled={executeCommand.isPending || !!activeRequestId}
              >
                {activeRequestId ? (
                  <Loader2 className="h-4 w-4 mr-1 animate-spin" />
                ) : (
                  <Play className="h-4 w-4 mr-1" />
                )}
                {activeRequestId ? 'Running...' : executeCommand.isPending ? 'Dispatching...' : 'Execute'}
              </Button>
            </div>

            {/* Dynamic parameter form */}
            {commandParams && commandParams.length > 0 && (
              <div className="border rounded-md p-3 space-y-3 bg-muted/30">
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Parameters
                </p>
                {commandParams.map((param) => (
                  <div key={param.id} className="space-y-1">
                    <Label className="text-sm">
                      {param.name}
                      {param.required && <span className="text-red-500 ml-0.5">*</span>}
                    </Label>
                    {param.description && (
                      <p className="text-xs text-muted-foreground">{param.description}</p>
                    )}
                    {renderParamField(param)}
                  </div>
                ))}
              </div>
            )}

            {/* Terminal output */}
            <div
              ref={scrollRef}
              className="bg-gray-950 text-gray-100 rounded-md p-4 font-mono text-xs flex-1 min-h-[200px] max-h-[300px] overflow-auto"
            >
              {output.length === 0 ? (
                <span className="text-gray-500">Output will appear here...</span>
              ) : (
                output.map((line, i) => (
                  <div key={i} className="whitespace-pre-wrap">
                    <span className="text-gray-500 mr-2">
                      {new Date(line.timestamp).toLocaleTimeString()}
                    </span>
                    <span
                      className={
                        line.type === 'stderr'
                          ? 'text-red-400'
                          : line.type === 'info'
                            ? 'text-blue-400'
                            : 'text-green-300'
                      }
                    >
                      {line.text}
                    </span>
                  </div>
                ))
              )}
              {activeRequestId && (
                <div className="flex items-center gap-2 mt-1 text-yellow-400">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  <span>Waiting for output...</span>
                </div>
              )}
            </div>
          </TabsContent>

          <TabsContent value="history" className="flex-1 overflow-auto">
            <div className="space-y-2">
              {!executions || executions.length === 0 ? (
                <p className="text-sm text-muted-foreground py-8 text-center">
                  No command execution history
                </p>
              ) : (
                executions.map((exec) => (
                  <div
                    key={exec.id}
                    className="border rounded-md p-3 space-y-1 text-sm"
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-medium">{exec.command_type}</span>
                      <span
                        className={`text-xs px-2 py-0.5 rounded-full ${
                          exec.status === 'completed'
                            ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                            : exec.status === 'failed'
                              ? 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
                              : 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200'
                        }`}
                      >
                        {exec.status}
                      </span>
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {new Date(exec.dispatched_at).toLocaleString()}
                      {exec.duration_ms != null && ` · ${exec.duration_ms}ms`}
                      {exec.exit_code != null && ` · exit ${exec.exit_code}`}
                    </div>
                    {exec.stdout && (
                      <pre className="text-xs bg-gray-950 text-green-300 p-2 rounded max-h-24 overflow-auto whitespace-pre-wrap">
                        {exec.stdout}
                      </pre>
                    )}
                    {exec.stderr && (
                      <pre className="text-xs bg-gray-950 text-red-400 p-2 rounded max-h-24 overflow-auto whitespace-pre-wrap">
                        {exec.stderr}
                      </pre>
                    )}
                  </div>
                ))
              )}
            </div>
          </TabsContent>
        </Tabs>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOutput([])}>
            Clear
          </Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
