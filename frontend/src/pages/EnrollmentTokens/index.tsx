import { useState, useEffect } from 'react';
import {
  useEnrollmentTokens,
  useCreateEnrollmentToken,
  useRevokeEnrollmentToken,
  useEnrollmentEvents,
  usePkiStatus,
  useInitPki,
  useImportPki,
  useRotationProgress,
  useStartRotation,
  useFinalizeRotation,
  useCancelRotation,
  getTokenStatus,
  type EnrollmentToken,
  type CreateEnrollmentTokenPayload,
  type CreateEnrollmentTokenResponse,
} from '@/api/enrollment';

// Fetch the latest release version from GitHub API
function useLatestReleaseVersion() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    fetch('https://api.github.com/repos/fredericcarre/appcontrol/releases')
      .then((res) => res.json())
      .then((releases: Array<{ tag_name: string }>) => {
        if (releases && releases.length > 0) {
          setVersion(releases[0].tag_name);
        }
      })
      .catch(() => {
        // Fallback to a default version
        setVersion('latest');
      });
  }, []);

  return version;
}
import { useGatewayZones } from '@/api/gateways';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@/components/ui/table';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  Plus,
  Key,
  Copy,
  Check,
  XCircle,
  Clock,
  Shield,
  AlertTriangle,
  CheckCircle,
  Upload,
  RefreshCw,
  RotateCcw,
} from 'lucide-react';
import { Progress } from '@/components/ui/progress';
import { Textarea } from '@/components/ui/textarea';

// ── Status badge helper ───────────────────────────────────────

type TokenStatus = 'active' | 'revoked' | 'expired' | 'exhausted';

function TokenStatusBadge({ status }: { status: TokenStatus }) {
  switch (status) {
    case 'active':
      return <Badge variant="running">Active</Badge>;
    case 'revoked':
      return <Badge variant="stopped">Revoked</Badge>;
    case 'expired':
      return <Badge variant="degraded">Expired</Badge>;
    case 'exhausted':
      return <Badge variant="stopped">Exhausted</Badge>;
    default:
      return <Badge variant="secondary">{status}</Badge>;
  }
}

// ── Command copy button ────────────────────────────────────────

function CopyCommandButton({ command, label }: { command: string; label: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleCopy}
          className="h-6 px-2 text-xs"
        >
          {copied ? (
            <>
              <Check className="h-3 w-3 mr-1 text-green-600" />
              Copied!
            </>
          ) : (
            <>
              <Copy className="h-3 w-3 mr-1" />
              Copy
            </>
          )}
        </Button>
      </div>
      <pre className="rounded-md border bg-muted px-3 py-2 text-xs font-mono break-all whitespace-pre-wrap select-all overflow-x-auto">
        {command}
      </pre>
    </div>
  );
}

// ── Created token display (shown once) ────────────────────────

function CreatedTokenDisplay({
  token,
  onClose,
}: {
  token: CreateEnrollmentTokenResponse;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [selectedOs, setSelectedOs] = useState<'linux' | 'macos' | 'windows'>('linux');
  const [selectedArch, setSelectedArch] = useState<'amd64' | 'arm64'>('amd64');

  // Fetch latest release version
  const latestVersion = useLatestReleaseVersion();

  // Get the current server URL for enrollment
  const serverHost = window.location.host;
  const isSecure = window.location.protocol === 'https:';

  // Binary download URLs (from GitHub releases)
  const binaryName = token.scope === 'gateway' ? 'appcontrol-gateway' : 'appcontrol-agent';
  // Use specific version tag instead of /latest/download which doesn't work for pre-releases
  const releaseBaseUrl = latestVersion
    ? `https://github.com/fredericcarre/appcontrol/releases/download/${latestVersion}`
    : 'https://github.com/fredericcarre/appcontrol/releases/latest/download';

  const getBinarySuffix = () => {
    if (selectedOs === 'windows') {
      return `${selectedArch === 'arm64' ? 'windows-arm64' : 'windows-amd64'}.exe`;
    }
    if (selectedOs === 'macos') {
      return selectedArch === 'arm64' ? 'darwin-arm64' : 'darwin-amd64';
    }
    return selectedArch === 'arm64' ? 'linux-arm64' : 'linux-amd64';
  };

  const binaryUrl = `${releaseBaseUrl}/${binaryName}-${getBinarySuffix()}`;
  const isWindows = selectedOs === 'windows';
  const ext = isWindows ? '.exe' : '';

  // Generate commands based on scope
  const generateCommands = () => {
    // Gateway URL for agent enrollment (uses mTLS on port 8443)
    const agentGatewayUrl = `wss://${serverHost}:8443`;
    // Backend URL for gateway connection
    const backendWsUrl = `${isSecure ? 'wss' : 'ws'}://${serverHost}/ws/gateway`;

    if (token.scope === 'gateway') {
      // Gateway doesn't have enrollment CLI - it uses config file
      const downloadCmd = isWindows
        ? `Invoke-WebRequest -Uri "${binaryUrl}" -OutFile "appcontrol-gateway${ext}"`
        : `curl -fsSL -o appcontrol-gateway "${binaryUrl}" && chmod +x appcontrol-gateway`;

      // Gateway config file creation (no enrollment CLI)
      const enrollCmd = isWindows
        ? `# Create gateway config file
@"
gateway:
  id: gateway-01
  name: Gateway 01
  zone: default
  listen_addr: 0.0.0.0
  listen_port: 4443
backend:
  url: ${backendWsUrl}
  reconnect_interval_secs: 5
"@ | Out-File -FilePath gateway.yaml -Encoding UTF8

# Start with config
.\\appcontrol-gateway${ext} --config gateway.yaml`
        : `# Create gateway config file
cat > gateway.yaml << 'EOF'
gateway:
  id: gateway-01
  name: Gateway 01
  zone: default
  listen_addr: 0.0.0.0
  listen_port: 4443
backend:
  url: ${backendWsUrl}
  reconnect_interval_secs: 5
EOF

# Start with config
./appcontrol-gateway --config gateway.yaml`;

      const serviceCmd = isWindows
        ? `# Install as Windows service (run as Administrator)
New-Service -Name "AppControlGateway" -BinaryPathName "$(Get-Location)\\appcontrol-gateway${ext} --config gateway.yaml" -StartupType Automatic
Start-Service AppControlGateway`
        : `# Install as systemd service (Linux)
sudo mv appcontrol-gateway /usr/local/bin/
sudo mkdir -p /etc/appcontrol
sudo mv gateway.yaml /etc/appcontrol/gateway.yaml
sudo tee /etc/systemd/system/appcontrol-gateway.service > /dev/null << 'EOF'
[Unit]
Description=AppControl Gateway
After=network.target

[Service]
ExecStart=/usr/local/bin/appcontrol-gateway --config /etc/appcontrol/gateway.yaml
Restart=always

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl enable --now appcontrol-gateway`;

      return { downloadCmd, enrollCmd, serviceCmd };
    } else {
      // Agent enrollment commands
      const downloadCmd = isWindows
        ? `Invoke-WebRequest -Uri "${binaryUrl}" -OutFile "appcontrol-agent${ext}"`
        : `curl -fsSL -o appcontrol-agent "${binaryUrl}" && chmod +x appcontrol-agent`;

      // Agent uses --enroll <url> --token <token> syntax
      const enrollCmd = isWindows
        ? `.\\appcontrol-agent${ext} --enroll "${agentGatewayUrl}" --token "${token.token}"`
        : `./appcontrol-agent --enroll "${agentGatewayUrl}" --token "${token.token}"`;

      const serviceCmd = isWindows
        ? `# Install as Windows service (run as Administrator)
New-Service -Name "AppControlAgent" -BinaryPathName "$(Get-Location)\\appcontrol-agent${ext} --config agent.yaml" -StartupType Automatic
Start-Service AppControlAgent`
        : `# Install as systemd service (Linux)
sudo mv appcontrol-agent /usr/local/bin/
sudo tee /etc/systemd/system/appcontrol-agent.service > /dev/null << 'EOF'
[Unit]
Description=AppControl Agent
After=network.target

[Service]
ExecStart=/usr/local/bin/appcontrol-agent --config /etc/appcontrol/agent.yaml
Restart=always

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl enable --now appcontrol-agent`;

      return { downloadCmd, enrollCmd, serviceCmd };
    }
  };

  const { downloadCmd, enrollCmd, serviceCmd } = generateCommands();

  const handleCopyToken = async () => {
    await navigator.clipboard.writeText(token.token);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {token.scope === 'gateway' ? 'Gateway' : 'Agent'} Enrollment Token Created
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-4">
          {/* Token display */}
          <div className="space-y-2">
            <p className="text-sm text-muted-foreground">
              Copy this token now. It will not be shown again.
            </p>
            <div className="flex items-center gap-2">
              <code className="flex-1 rounded-md border bg-muted px-3 py-2 text-sm font-mono break-all select-all">
                {token.token}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={handleCopyToken}
                aria-label="Copy token"
              >
                {copied ? (
                  <Check className="h-4 w-4 text-green-600" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>

          {/* Token details */}
          <div className="text-sm grid grid-cols-2 gap-2 p-3 rounded-md bg-muted/50">
            <p>
              <span className="text-muted-foreground">Name:</span> {token.name}
            </p>
            <p>
              <span className="text-muted-foreground">Scope:</span> {token.scope}
            </p>
            <p>
              <span className="text-muted-foreground">Max uses:</span>{' '}
              {token.max_uses ?? 'Unlimited'}
            </p>
            <p>
              <span className="text-muted-foreground">Expires:</span>{' '}
              {token.expires_at
                ? new Date(token.expires_at).toLocaleString()
                : 'Never'}
            </p>
          </div>

          {/* OS/Arch selector */}
          <div className="flex items-center gap-4 pt-2">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">Platform:</span>
              <Select value={selectedOs} onValueChange={(v) => setSelectedOs(v as 'linux' | 'macos' | 'windows')}>
                <SelectTrigger className="w-28 h-8">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="linux">Linux</SelectItem>
                  <SelectItem value="macos">macOS</SelectItem>
                  <SelectItem value="windows">Windows</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">Arch:</span>
              <Select value={selectedArch} onValueChange={(v) => setSelectedArch(v as 'amd64' | 'arm64')}>
                <SelectTrigger className="w-24 h-8">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="amd64">x64</SelectItem>
                  <SelectItem value="arm64">ARM64</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {/* Commands */}
          <div className="space-y-3 pt-2">
            <CopyCommandButton
              label="1. Download binary"
              command={downloadCmd}
            />
            <CopyCommandButton
              label="2. Enroll"
              command={enrollCmd}
            />
            <CopyCommandButton
              label="3. Install as service (optional)"
              command={serviceCmd}
            />
          </div>
        </div>
        <DialogFooter>
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Create token dialog ───────────────────────────────────────

function CreateTokenDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: (token: CreateEnrollmentTokenResponse) => void;
}) {
  const createToken = useCreateEnrollmentToken();
  const { data: zones } = useGatewayZones();
  const [name, setName] = useState('');
  const [scope, setScope] = useState<'agent' | 'gateway'>('agent');
  const [zone, setZone] = useState<string>('');
  const [maxUses, setMaxUses] = useState('');
  const [validHours, setValidHours] = useState('24');

  // Get unique zones from gateway data
  const availableZones = zones?.map((z) => z.zone) ?? [];

  const handleCreate = async () => {
    if (!name.trim()) return;

    const payload: CreateEnrollmentTokenPayload = {
      name: name.trim(),
      scope,
      valid_hours: validHours ? parseInt(validHours, 10) : 24,
    };
    if (maxUses) {
      payload.max_uses = parseInt(maxUses, 10);
    }
    if (zone) {
      payload.zone = zone;
    }

    const result = await createToken.mutateAsync(payload);
    onCreated(result);
    resetForm();
    onOpenChange(false);
  };

  const resetForm = () => {
    setName('');
    setScope('agent');
    setZone('');
    setMaxUses('');
    setValidHours('24');
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create Enrollment Token</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">Name</label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. production-agents-batch1"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Scope</label>
            <Select value={scope} onValueChange={(v) => setScope(v as 'agent' | 'gateway')}>
              <SelectTrigger>
                <SelectValue placeholder="Select scope" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="agent">Agent</SelectItem>
                <SelectItem value="gateway">Gateway</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">
              Zone{' '}
              <span className="text-muted-foreground font-normal">
                (optional, restricts enrollment to this zone)
              </span>
            </label>
            <Select value={zone} onValueChange={setZone}>
              <SelectTrigger>
                <SelectValue placeholder="Any zone (unrestricted)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">Any zone (unrestricted)</SelectItem>
                {availableZones.map((z) => (
                  <SelectItem key={z} value={z}>
                    {z}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              If set, agents can only enroll via gateways in this zone
            </p>
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">
              Max Uses{' '}
              <span className="text-muted-foreground font-normal">
                (optional, leave blank for unlimited)
              </span>
            </label>
            <Input
              type="number"
              min="1"
              value={maxUses}
              onChange={(e) => setMaxUses(e.target.value)}
              placeholder="Unlimited"
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Valid Hours</label>
            <Input
              type="number"
              min="1"
              value={validHours}
              onChange={(e) => setValidHours(e.target.value)}
              placeholder="24"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            onClick={handleCreate}
            disabled={!name.trim() || createToken.isPending}
          >
            {createToken.isPending ? 'Creating...' : 'Create Token'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Revoke confirmation dialog ────────────────────────────────

function RevokeDialog({
  token,
  onOpenChange,
}: {
  token: EnrollmentToken | null;
  onOpenChange: (open: boolean) => void;
}) {
  const revokeToken = useRevokeEnrollmentToken();

  const handleRevoke = async () => {
    if (!token) return;
    await revokeToken.mutateAsync(token.id);
    onOpenChange(false);
  };

  return (
    <Dialog open={!!token} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Revoke Token</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <p className="text-sm text-muted-foreground">
            Are you sure you want to revoke the token{' '}
            <span className="font-medium text-foreground">
              {token?.name}
            </span>
            ? This action cannot be undone. Any agents or gateways that have not
            yet enrolled with this token will no longer be able to use it.
          </p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleRevoke}
            disabled={revokeToken.isPending}
          >
            {revokeToken.isPending ? 'Revoking...' : 'Revoke Token'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Tokens table ──────────────────────────────────────────────

function TokensTable({
  tokens,
  onRevoke,
}: {
  tokens: EnrollmentToken[];
  onRevoke: (token: EnrollmentToken) => void;
}) {
  return (
    <Card>
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Scope</TableHead>
              <TableHead>Uses</TableHead>
              <TableHead>Expires</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Created</TableHead>
              <TableHead className="w-[100px]">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {!tokens.length ? (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="text-center text-muted-foreground py-8"
                >
                  No enrollment tokens yet
                </TableCell>
              </TableRow>
            ) : (
              tokens.map((token) => {
                const status = getTokenStatus(token);
                return (
                  <TableRow key={token.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Key className="h-4 w-4 text-muted-foreground" />
                        <span className="font-medium">{token.name}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{token.scope}</Badge>
                    </TableCell>
                    <TableCell>
                      <span className="text-sm">
                        {token.current_uses}
                        {token.max_uses != null ? ` / ${token.max_uses}` : ''}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground text-sm">
                      {token.expires_at ? (
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {new Date(token.expires_at).toLocaleString()}
                        </span>
                      ) : (
                        'Never'
                      )}
                    </TableCell>
                    <TableCell>
                      <TokenStatusBadge status={status} />
                    </TableCell>
                    <TableCell className="text-muted-foreground text-sm">
                      {new Date(token.created_at).toLocaleDateString()}
                    </TableCell>
                    <TableCell>
                      {status === 'active' && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => onRevoke(token)}
                          className="text-destructive hover:text-destructive"
                        >
                          <XCircle className="h-4 w-4 mr-1" />
                          Revoke
                        </Button>
                      )}
                    </TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

// ── Events table ──────────────────────────────────────────────

function EventsTable() {
  const { data: events, isLoading } = useEnrollmentEvents();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-32">
        <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <Card>
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Time</TableHead>
              <TableHead>Token</TableHead>
              <TableHead>Event</TableHead>
              <TableHead>Hostname</TableHead>
              <TableHead>IP Address</TableHead>
              <TableHead>Details</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {!events?.length ? (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="text-center text-muted-foreground py-8"
                >
                  No enrollment events recorded
                </TableCell>
              </TableRow>
            ) : (
              events.map((event) => (
                <TableRow key={event.id}>
                  <TableCell className="text-sm text-muted-foreground whitespace-nowrap">
                    {new Date(event.created_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <span className="font-medium">{event.token_name}</span>
                  </TableCell>
                  <TableCell>
                    <Badge variant="outline">{event.event_type}</Badge>
                  </TableCell>
                  <TableCell className="text-sm">
                    {event.hostname || '-'}
                  </TableCell>
                  <TableCell className="text-sm font-mono">
                    {event.ip_address || '-'}
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground max-w-[200px] truncate">
                    {Object.keys(event.details).length > 0
                      ? JSON.stringify(event.details)
                      : '-'}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

// ── PKI initialization card ────────────────────────────────────

function PkiInitCard() {
  const { data: pkiStatus, isLoading } = usePkiStatus();
  const initPki = useInitPki();
  const importPki = useImportPki();
  const [orgName, setOrgName] = useState('');
  const [showInit, setShowInit] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [caCertPem, setCaCertPem] = useState('');
  const [caKeyPem, setCaKeyPem] = useState('');
  const [importError, setImportError] = useState('');

  if (isLoading) {
    return null;
  }

  if (pkiStatus?.initialized) {
    return (
      <Card className="border-green-200 bg-green-50 dark:border-green-900 dark:bg-green-950">
        <CardContent className="flex items-start gap-3 pt-4">
          <CheckCircle className="h-5 w-5 text-green-600 mt-0.5 shrink-0" />
          <div>
            <p className="font-medium text-green-800 dark:text-green-200">PKI Initialized</p>
            <p className="text-sm text-green-700 dark:text-green-300">
              <span className="text-muted-foreground">CA Fingerprint: </span>
              <code className="text-xs font-mono">{pkiStatus.ca_fingerprint}</code>
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  const handleInit = async () => {
    if (!orgName.trim()) return;
    await initPki.mutateAsync({ org_name: orgName.trim() });
    setShowInit(false);
    setOrgName('');
  };

  const handleImport = async () => {
    setImportError('');
    if (!caCertPem.trim() || !caKeyPem.trim()) {
      setImportError('Both certificate and private key are required');
      return;
    }
    try {
      await importPki.mutateAsync({
        ca_cert_pem: caCertPem.trim(),
        ca_key_pem: caKeyPem.trim(),
      });
      setShowImport(false);
      setCaCertPem('');
      setCaKeyPem('');
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { message?: string } } };
      setImportError(axiosErr.response?.data?.message || 'Failed to import CA');
    }
  };

  const closeImportDialog = () => {
    setShowImport(false);
    setCaCertPem('');
    setCaKeyPem('');
    setImportError('');
  };

  return (
    <>
      <Card className="border-yellow-300 bg-yellow-50 dark:border-yellow-900 dark:bg-yellow-950">
        <CardContent className="flex items-start gap-3 pt-4">
          <AlertTriangle className="h-5 w-5 text-yellow-600 mt-0.5 shrink-0" />
          <div className="flex-1">
            <p className="font-medium text-yellow-800 dark:text-yellow-200">PKI Not Initialized</p>
            <p className="text-sm text-yellow-700 dark:text-yellow-300 mt-1">
              You must initialize the PKI before agents or gateways can enroll.
              Generate a new CA or import your enterprise CA.
            </p>
            <div className="mt-3 flex gap-2">
              <Button size="sm" onClick={() => setShowInit(true)}>
                Generate CA
              </Button>
              <Button size="sm" variant="outline" onClick={() => setShowImport(true)}>
                <Upload className="h-4 w-4 mr-1" />
                Import CA
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Generate CA Dialog */}
      <Dialog open={showInit} onOpenChange={setShowInit}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Generate Certificate Authority</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <p className="text-sm text-muted-foreground">
              This will create a new self-signed Certificate Authority (CA) for your organization.
              The CA will be used to sign certificates for agents and gateways.
            </p>
            <div className="space-y-2">
              <label className="text-sm font-medium">Organization Name</label>
              <Input
                value={orgName}
                onChange={(e) => setOrgName(e.target.value)}
                placeholder="e.g. Acme Corp"
              />
              <p className="text-xs text-muted-foreground">
                This name will appear in the CA certificate
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowInit(false)}>
              Cancel
            </Button>
            <Button
              onClick={handleInit}
              disabled={!orgName.trim() || initPki.isPending}
            >
              {initPki.isPending ? 'Generating...' : 'Generate CA'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Import CA Dialog */}
      <Dialog open={showImport} onOpenChange={(open) => !open && closeImportDialog()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Import Enterprise CA</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <p className="text-sm text-muted-foreground">
              Import your existing enterprise Certificate Authority. All agent and gateway
              certificates will be signed by this CA, enabling seamless mTLS with your infrastructure.
            </p>
            {importError && (
              <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
                <AlertTriangle className="h-4 w-4 shrink-0" />
                {importError}
              </div>
            )}
            <div className="space-y-2">
              <label className="text-sm font-medium">CA Certificate (PEM)</label>
              <Textarea
                value={caCertPem}
                onChange={(e) => setCaCertPem(e.target.value)}
                placeholder="-----BEGIN CERTIFICATE-----&#10;...&#10;-----END CERTIFICATE-----"
                className="font-mono text-xs h-32"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">CA Private Key (PEM)</label>
              <Textarea
                value={caKeyPem}
                onChange={(e) => setCaKeyPem(e.target.value)}
                placeholder="-----BEGIN PRIVATE KEY-----&#10;...&#10;-----END PRIVATE KEY-----"
                className="font-mono text-xs h-32"
              />
              <p className="text-xs text-muted-foreground">
                The private key is stored encrypted and never leaves the server.
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={closeImportDialog}>
              Cancel
            </Button>
            <Button
              onClick={handleImport}
              disabled={!caCertPem.trim() || !caKeyPem.trim() || importPki.isPending}
            >
              {importPki.isPending ? 'Importing...' : 'Import CA'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// ── Certificate Rotation Card ───────────────────────────────────

function CertificateRotationCard() {
  const { data: pkiStatus } = usePkiStatus();
  const { data: progress } = useRotationProgress();
  const startRotation = useStartRotation();
  const finalizeRotation = useFinalizeRotation();
  const cancelRotation = useCancelRotation();

  const [showStartDialog, setShowStartDialog] = useState(false);
  const [newCaCertPem, setNewCaCertPem] = useState('');
  const [newCaKeyPem, setNewCaKeyPem] = useState('');
  const [gracePeriodHours, setGracePeriodHours] = useState('1');
  const [startError, setStartError] = useState('');

  // Don't show if PKI not initialized
  if (!pkiStatus?.initialized) {
    return null;
  }

  const handleStartRotation = async () => {
    setStartError('');
    if (!newCaCertPem.trim() || !newCaKeyPem.trim()) {
      setStartError('Both certificate and private key are required');
      return;
    }
    try {
      const gracePeriodSecs = (parseInt(gracePeriodHours, 10) || 1) * 3600;
      await startRotation.mutateAsync({
        new_ca_cert_pem: newCaCertPem.trim(),
        new_ca_key_pem: newCaKeyPem.trim(),
        grace_period_secs: gracePeriodSecs,
      });
      setShowStartDialog(false);
      setNewCaCertPem('');
      setNewCaKeyPem('');
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { message?: string } } };
      setStartError(axiosErr.response?.data?.message || 'Failed to start rotation');
    }
  };

  const closeStartDialog = () => {
    setShowStartDialog(false);
    setNewCaCertPem('');
    setNewCaKeyPem('');
    setStartError('');
  };

  // Calculate progress percentage
  const totalEntities = (progress?.total_agents ?? 0) + (progress?.total_gateways ?? 0);
  const migratedEntities = (progress?.migrated_agents ?? 0) + (progress?.migrated_gateways ?? 0);
  const progressPercent = totalEntities > 0 ? Math.round((migratedEntities / totalEntities) * 100) : 0;

  // Show active rotation progress
  if (progress && (progress.status === 'in_progress' || progress.status === 'ready')) {
    return (
      <Card className="border-blue-200 bg-blue-50 dark:border-blue-900 dark:bg-blue-950">
        <CardContent className="pt-4">
          <div className="flex items-start gap-3">
            <RefreshCw className="h-5 w-5 text-blue-600 mt-0.5 shrink-0 animate-spin" />
            <div className="flex-1 space-y-4">
              <div>
                <p className="font-medium text-blue-800 dark:text-blue-200">
                  Certificate Rotation {progress.status === 'ready' ? 'Ready to Finalize' : 'In Progress'}
                </p>
                <p className="text-sm text-blue-700 dark:text-blue-300 mt-1">
                  Migrating from <code className="text-xs">{progress.old_ca_fingerprint?.slice(0, 16)}...</code>
                  {' → '}
                  <code className="text-xs">{progress.new_ca_fingerprint?.slice(0, 16)}...</code>
                </p>
              </div>

              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>Progress</span>
                  <span>{migratedEntities} / {totalEntities} entities migrated</span>
                </div>
                <Progress value={progressPercent} className="h-2" />
              </div>

              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <span className="text-muted-foreground">Agents:</span>{' '}
                  {progress.migrated_agents} / {progress.total_agents}
                  {progress.failed_agents > 0 && (
                    <span className="text-destructive ml-1">({progress.failed_agents} failed)</span>
                  )}
                </div>
                <div>
                  <span className="text-muted-foreground">Gateways:</span>{' '}
                  {progress.migrated_gateways} / {progress.total_gateways}
                  {progress.failed_gateways > 0 && (
                    <span className="text-destructive ml-1">({progress.failed_gateways} failed)</span>
                  )}
                </div>
              </div>

              <div className="flex gap-2 pt-2">
                {progress.status === 'ready' && (
                  <Button
                    size="sm"
                    onClick={() => finalizeRotation.mutate()}
                    disabled={finalizeRotation.isPending}
                  >
                    {finalizeRotation.isPending ? 'Finalizing...' : 'Finalize Rotation'}
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => cancelRotation.mutate()}
                  disabled={cancelRotation.isPending}
                >
                  <XCircle className="h-4 w-4 mr-1" />
                  {cancelRotation.isPending ? 'Cancelling...' : 'Cancel'}
                </Button>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Show completed rotation status briefly
  if (progress?.status === 'completed') {
    return (
      <Card className="border-green-200 bg-green-50 dark:border-green-900 dark:bg-green-950">
        <CardContent className="flex items-start gap-3 pt-4">
          <CheckCircle className="h-5 w-5 text-green-600 mt-0.5 shrink-0" />
          <div>
            <p className="font-medium text-green-800 dark:text-green-200">
              Certificate Rotation Completed
            </p>
            <p className="text-sm text-green-700 dark:text-green-300">
              All entities have been migrated to the new CA.
              {progress.finalized_at && (
                <> Finalized {new Date(progress.finalized_at).toLocaleString()}</>
              )}
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Show option to start rotation
  return (
    <>
      <Card>
        <CardContent className="flex items-start gap-3 pt-4">
          <RotateCcw className="h-5 w-5 text-muted-foreground mt-0.5 shrink-0" />
          <div className="flex-1">
            <div className="flex items-center justify-between">
              <div>
                <p className="font-medium">Certificate Rotation</p>
                <p className="text-sm text-muted-foreground mt-1">
                  Rotate to a new CA certificate with zero downtime.
                  {pkiStatus.enrolled_agents !== undefined && (
                    <> {pkiStatus.enrolled_agents} agents and {pkiStatus.enrolled_gateways} gateways enrolled.</>
                  )}
                </p>
              </div>
              <Button size="sm" variant="outline" onClick={() => setShowStartDialog(true)}>
                <RefreshCw className="h-4 w-4 mr-1" />
                Start Rotation
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Start Rotation Dialog */}
      <Dialog open={showStartDialog} onOpenChange={(open) => !open && closeStartDialog()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Start Certificate Rotation</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <p className="text-sm text-muted-foreground">
              Import the new CA certificate that will replace the current one.
              During rotation, both old and new CAs are trusted, allowing gradual migration.
            </p>
            {startError && (
              <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive text-sm">
                <AlertTriangle className="h-4 w-4 shrink-0" />
                {startError}
              </div>
            )}
            <div className="space-y-2">
              <label className="text-sm font-medium">New CA Certificate (PEM)</label>
              <Textarea
                value={newCaCertPem}
                onChange={(e) => setNewCaCertPem(e.target.value)}
                placeholder="-----BEGIN CERTIFICATE-----&#10;...&#10;-----END CERTIFICATE-----"
                className="font-mono text-xs h-32"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">New CA Private Key (PEM)</label>
              <Textarea
                value={newCaKeyPem}
                onChange={(e) => setNewCaKeyPem(e.target.value)}
                placeholder="-----BEGIN PRIVATE KEY-----&#10;...&#10;-----END PRIVATE KEY-----"
                className="font-mono text-xs h-32"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">Grace Period (hours)</label>
              <Input
                type="number"
                min="1"
                value={gracePeriodHours}
                onChange={(e) => setGracePeriodHours(e.target.value)}
                placeholder="1"
                className="w-32"
              />
              <p className="text-xs text-muted-foreground">
                How long to wait before considering the old CA invalid
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={closeStartDialog}>
              Cancel
            </Button>
            <Button
              onClick={handleStartRotation}
              disabled={!newCaCertPem.trim() || !newCaKeyPem.trim() || startRotation.isPending}
            >
              {startRotation.isPending ? 'Starting...' : 'Start Rotation'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// ── Main page ─────────────────────────────────────────────────

export function EnrollmentTokensPage() {
  const { data: tokens, isLoading } = useEnrollmentTokens();
  const [createOpen, setCreateOpen] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<EnrollmentToken | null>(null);
  const [createdToken, setCreatedToken] =
    useState<CreateEnrollmentTokenResponse | null>(null);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Shield className="h-6 w-6 text-primary" />
          <h1 className="text-2xl font-bold">Enrollment Tokens</h1>
        </div>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus className="h-4 w-4 mr-2" /> Create Token
        </Button>
      </div>

      <PkiInitCard />

      <CertificateRotationCard />

      <Tabs defaultValue="tokens">
        <TabsList>
          <TabsTrigger value="tokens">Tokens</TabsTrigger>
          <TabsTrigger value="events">Enrollment Events</TabsTrigger>
        </TabsList>

        <TabsContent value="tokens">
          <TokensTable
            tokens={tokens || []}
            onRevoke={(token) => setRevokeTarget(token)}
          />
        </TabsContent>

        <TabsContent value="events">
          <EventsTable />
        </TabsContent>
      </Tabs>

      <CreateTokenDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={setCreatedToken}
      />

      <RevokeDialog
        token={revokeTarget}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null);
        }}
      />

      {createdToken && (
        <CreatedTokenDisplay
          token={createdToken}
          onClose={() => setCreatedToken(null)}
        />
      )}
    </div>
  );
}
