import { useState, useCallback } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { AgentTerminal } from './AgentTerminal';
import { Terminal, X } from 'lucide-react';

interface TerminalModalProps {
  agentId: string;
  agentHostname: string;
  agentOs?: string | null;
  open: boolean;
  onClose: () => void;
}

// Detect if agent is running Windows
function isWindowsAgent(os?: string | null): boolean {
  if (!os) return false;
  const osLower = os.toLowerCase();
  return osLower.includes('windows');
}

// Get default shell based on OS
function getDefaultShell(os?: string | null): string {
  if (isWindowsAgent(os)) {
    return 'cmd.exe';
  }
  return '/bin/bash';
}

// Get shell options based on OS
function getShellOptions(os?: string | null): { value: string; label: string }[] {
  if (isWindowsAgent(os)) {
    return [
      { value: 'cmd.exe', label: 'cmd' },
      { value: 'powershell.exe', label: 'PowerShell' },
      { value: 'pwsh.exe', label: 'PowerShell 7' },
    ];
  }
  return [
    { value: '/bin/bash', label: 'bash' },
    { value: '/bin/sh', label: 'sh' },
    { value: '/bin/zsh', label: 'zsh' },
  ];
}

export function TerminalModal({
  agentId,
  agentHostname,
  agentOs,
  open,
  onClose,
}: TerminalModalProps) {
  const [shell, setShell] = useState<string>(() => getDefaultShell(agentOs));
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  const handleSessionStart = useCallback((serverSessionId: string) => {
    // Use the session ID from the server
    setSessionId(serverSessionId);
    setIsConnected(true);
  }, []);

  const handleSessionEnd = useCallback(() => {
    setSessionId(null);
    setIsConnected(false);
  }, []);

  const handleClose = () => {
    if (isConnected) {
      // The WebSocket close will handle cleanup
    }
    setSessionId(null);
    setIsConnected(false);
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className="max-w-4xl h-[600px] flex flex-col p-0 gap-0">
        <DialogHeader className="px-4 py-3 border-b flex flex-row items-center justify-between space-y-0">
          <div className="flex items-center gap-3">
            <Terminal className="h-5 w-5 text-muted-foreground" />
            <DialogTitle className="text-lg">
              Terminal: {agentHostname}
            </DialogTitle>
          </div>
          <div className="flex items-center gap-2">
            {!isConnected && (
              <Select value={shell} onValueChange={setShell}>
                <SelectTrigger className="w-[140px] h-8">
                  <SelectValue placeholder="Shell" />
                </SelectTrigger>
                <SelectContent>
                  {getShellOptions(agentOs).map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
            <Button variant="ghost" size="icon" className="h-8 w-8" onClick={handleClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </DialogHeader>
        <div className="flex-1 min-h-0">
          <AgentTerminal
            agentId={agentId}
            sessionId={sessionId}
            onSessionStart={handleSessionStart}
            onSessionEnd={handleSessionEnd}
            shell={shell}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}
