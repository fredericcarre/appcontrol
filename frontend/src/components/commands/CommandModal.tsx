import { useState, useRef, useEffect } from 'react';
import { useExecuteCommand } from '@/api/components';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '@/components/ui/select';
import { Play, Terminal } from 'lucide-react';

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
  const [commandType, setCommandType] = useState('check');
  const [customArgs, setCustomArgs] = useState('');
  const [output, setOutput] = useState<OutputLine[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [output]);

  const handleExecute = async () => {
    const now = new Date().toISOString();
    setOutput((prev) => [...prev, { text: `> Executing ${commandType}...`, timestamp: now, type: 'info' }]);

    try {
      const result = await executeCommand.mutateAsync({
        component_id: componentId,
        command_type: commandType,
        args: customArgs ? customArgs.split(' ') : undefined,
      });

      setOutput((prev) => [
        ...prev,
        { text: result.output || 'Command completed', timestamp: new Date().toISOString(), type: 'stdout' },
        { text: `Exit code: ${result.exit_code ?? 'N/A'}`, timestamp: new Date().toISOString(), type: 'info' },
      ]);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : 'Command failed';
      setOutput((prev) => [
        ...prev,
        { text: message, timestamp: new Date().toISOString(), type: 'stderr' },
      ]);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Terminal className="h-5 w-5" /> Execute Command
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="flex gap-2">
            <Select value={commandType} onValueChange={setCommandType}>
              <SelectTrigger className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="check">Health Check</SelectItem>
                <SelectItem value="start">Start</SelectItem>
                <SelectItem value="stop">Stop</SelectItem>
                <SelectItem value="restart">Restart</SelectItem>
                <SelectItem value="integrity_check">Integrity Check</SelectItem>
                <SelectItem value="infra_check">Infra Check</SelectItem>
                <SelectItem value="custom">Custom</SelectItem>
              </SelectContent>
            </Select>

            {commandType === 'custom' && (
              <Input
                placeholder="Command arguments..."
                value={customArgs}
                onChange={(e) => setCustomArgs(e.target.value)}
                className="flex-1"
              />
            )}

            <Button onClick={handleExecute} disabled={executeCommand.isPending}>
              <Play className="h-4 w-4 mr-1" />
              {executeCommand.isPending ? 'Running...' : 'Execute'}
            </Button>
          </div>

          <div
            ref={scrollRef}
            className="bg-gray-950 text-gray-100 rounded-md p-4 font-mono text-xs h-[300px] overflow-auto"
          >
            {output.length === 0 ? (
              <span className="text-gray-500">Output will appear here...</span>
            ) : (
              output.map((line, i) => (
                <div key={i} className="whitespace-pre-wrap">
                  <span className="text-gray-500 mr-2">
                    {new Date(line.timestamp).toLocaleTimeString()}
                  </span>
                  <span className={
                    line.type === 'stderr' ? 'text-red-400' :
                    line.type === 'info' ? 'text-blue-400' :
                    'text-green-300'
                  }>
                    {line.text}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOutput([])}>Clear</Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
