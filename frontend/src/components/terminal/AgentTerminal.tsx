import { useEffect, useRef, useCallback, useState } from 'react';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import 'xterm/css/xterm.css';
import { useAuthStore } from '@/stores/auth';
import { Badge } from '@/components/ui/badge';
import { Wifi, WifiOff, Loader2 } from 'lucide-react';

interface AgentTerminalProps {
  agentId: string;
  sessionId: string | null;
  onSessionStart: () => void;
  onSessionEnd: () => void;
  shell?: string;
}

type ConnectionStatus = 'disconnected' | 'connecting' | 'connected';

export function AgentTerminal({
  agentId,
  sessionId,
  onSessionStart,
  onSessionEnd,
  shell,
}: AgentTerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>('disconnected');
  const token = useAuthStore((s) => s.token);

  // Initialize terminal
  useEffect(() => {
    if (!terminalRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      fontSize: 14,
      theme: {
        background: '#1a1a2e',
        foreground: '#eaeaea',
        cursor: '#f8f8f2',
        cursorAccent: '#1a1a2e',
        selectionBackground: 'rgba(248, 248, 242, 0.3)',
        black: '#21222c',
        red: '#ff5555',
        green: '#50fa7b',
        yellow: '#f1fa8c',
        blue: '#6272a4',
        magenta: '#ff79c6',
        cyan: '#8be9fd',
        white: '#f8f8f2',
        brightBlack: '#6272a4',
        brightRed: '#ff6e6e',
        brightGreen: '#69ff94',
        brightYellow: '#ffffa5',
        brightBlue: '#d6acff',
        brightMagenta: '#ff92df',
        brightCyan: '#a4ffff',
        brightWhite: '#ffffff',
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef.current);
    fitAddon.fit();

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    // Welcome message
    term.writeln('AppControl Terminal');
    term.writeln('───────────────────────────────────────');
    term.writeln(`Agent: ${agentId}`);
    term.writeln('');
    term.writeln('Connecting...');

    // Handle resize
    const handleResize = () => {
      if (fitAddonRef.current) {
        fitAddonRef.current.fit();
        // Send resize to server
        if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN && sessionId) {
          const cols = xtermRef.current?.cols || 80;
          const rows = xtermRef.current?.rows || 24;
          wsRef.current.send(
            JSON.stringify({
              type: 'TerminalResize',
              payload: { session_id: sessionId, cols, rows },
            })
          );
        }
      }
    };
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(terminalRef.current);

    return () => {
      resizeObserver.disconnect();
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId]);

  // Handle incoming WebSocket messages
  const handleWsMessage = useCallback(
    (msg: { type: string; payload: Record<string, unknown> }) => {
      switch (msg.type) {
        case 'TerminalStarted':
          setStatus('connected');
          if (xtermRef.current) {
            xtermRef.current.clear();
            xtermRef.current.focus();
          }
          onSessionStart();
          break;

        case 'TerminalOutput':
          if (xtermRef.current && msg.payload.data) {
            // Decode base64 data
            const decoded = atob(msg.payload.data as string);
            xtermRef.current.write(decoded);
          }
          break;

        case 'TerminalExit':
          setStatus('disconnected');
          if (xtermRef.current) {
            const exitCode = msg.payload.exit_code as number;
            xtermRef.current.writeln(`\r\n\x1b[33mSession ended (exit code: ${exitCode})\x1b[0m`);
          }
          onSessionEnd();
          break;

        case 'TerminalError':
          setStatus('disconnected');
          if (xtermRef.current) {
            xtermRef.current.writeln(`\r\n\x1b[31mError: ${msg.payload.error}\x1b[0m`);
          }
          onSessionEnd();
          break;
      }
    },
    [onSessionStart, onSessionEnd]
  );

  // WebSocket connection
  useEffect(() => {
    if (!token) return;

    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${wsProtocol}//${window.location.host}/ws?token=${token}`;

    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;
    setStatus('connecting');

    ws.onopen = () => {
      // Start terminal session
      const cols = xtermRef.current?.cols || 80;
      const rows = xtermRef.current?.rows || 24;
      ws.send(
        JSON.stringify({
          type: 'TerminalStart',
          payload: {
            agent_id: agentId,
            shell: shell || null,
            cols,
            rows,
          },
        })
      );
    };

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        handleWsMessage(msg);
      } catch {
        // Ignore non-JSON messages
      }
    };

    ws.onclose = () => {
      setStatus('disconnected');
      if (xtermRef.current) {
        xtermRef.current.writeln('\r\n\x1b[31mConnection closed\x1b[0m');
      }
      onSessionEnd();
    };

    ws.onerror = () => {
      setStatus('disconnected');
      if (xtermRef.current) {
        xtermRef.current.writeln('\r\n\x1b[31mConnection error\x1b[0m');
      }
    };

    return () => {
      ws.close();
    };
  }, [token, agentId, shell, handleWsMessage, onSessionEnd]);

  // Handle user input
  useEffect(() => {
    if (!xtermRef.current || !sessionId) return;

    const disposable = xtermRef.current.onData((data) => {
      if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
        // Encode to base64
        const encoded = btoa(data);
        wsRef.current.send(
          JSON.stringify({
            type: 'TerminalInput',
            payload: { session_id: sessionId, data: encoded },
          })
        );
      }
    });

    return () => {
      disposable.dispose();
    };
  }, [sessionId]);

  const statusBadge = () => {
    switch (status) {
      case 'connecting':
        return (
          <Badge variant="outline" className="gap-1">
            <Loader2 className="h-3 w-3 animate-spin" />
            Connecting
          </Badge>
        );
      case 'connected':
        return (
          <Badge variant="running" className="gap-1">
            <Wifi className="h-3 w-3" />
            Connected
          </Badge>
        );
      default:
        return (
          <Badge variant="stopped" className="gap-1">
            <WifiOff className="h-3 w-3" />
            Disconnected
          </Badge>
        );
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 bg-[#1a1a2e] border-b border-gray-700">
        <span className="text-sm text-gray-400 font-mono">
          {agentId.slice(0, 8)}...
        </span>
        {statusBadge()}
      </div>
      <div ref={terminalRef} className="flex-1 bg-[#1a1a2e]" />
    </div>
  );
}
