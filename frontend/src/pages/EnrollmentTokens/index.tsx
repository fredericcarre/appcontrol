import { useState } from 'react';
import {
  useEnrollmentTokens,
  useCreateEnrollmentToken,
  useRevokeEnrollmentToken,
  useEnrollmentEvents,
  usePkiStatus,
  useInitPki,
  getTokenStatus,
  type EnrollmentToken,
  type CreateEnrollmentTokenPayload,
  type CreateEnrollmentTokenResponse,
} from '@/api/enrollment';
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
} from 'lucide-react';

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

// ── Created token display (shown once) ────────────────────────

function CreatedTokenDisplay({
  token,
  onClose,
}: {
  token: CreateEnrollmentTokenResponse;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(token.token);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Dialog open onOpenChange={onClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Token Created</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-4">
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
              onClick={handleCopy}
              aria-label="Copy token"
            >
              {copied ? (
                <Check className="h-4 w-4 text-green-600" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </Button>
          </div>
          <div className="text-sm space-y-1">
            <p>
              <span className="font-medium">Name:</span> {token.name}
            </p>
            <p>
              <span className="font-medium">Scope:</span> {token.scope}
            </p>
            <p>
              <span className="font-medium">Max uses:</span>{' '}
              {token.max_uses ?? 'Unlimited'}
            </p>
            <p>
              <span className="font-medium">Expires:</span>{' '}
              {token.expires_at
                ? new Date(token.expires_at).toLocaleString()
                : 'Never'}
            </p>
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
  const [name, setName] = useState('');
  const [scope, setScope] = useState<'agent' | 'gateway'>('agent');
  const [maxUses, setMaxUses] = useState('');
  const [validHours, setValidHours] = useState('24');

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

    const result = await createToken.mutateAsync(payload);
    onCreated(result);
    resetForm();
    onOpenChange(false);
  };

  const resetForm = () => {
    setName('');
    setScope('agent');
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
  const [orgName, setOrgName] = useState('');
  const [showInit, setShowInit] = useState(false);

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

  return (
    <>
      <Card className="border-yellow-300 bg-yellow-50 dark:border-yellow-900 dark:bg-yellow-950">
        <CardContent className="flex items-start gap-3 pt-4">
          <AlertTriangle className="h-5 w-5 text-yellow-600 mt-0.5 shrink-0" />
          <div className="flex-1">
            <p className="font-medium text-yellow-800 dark:text-yellow-200">PKI Not Initialized</p>
            <p className="text-sm text-yellow-700 dark:text-yellow-300 mt-1">
              You must initialize the PKI before agents or gateways can enroll. This creates the Certificate Authority for your organization.
            </p>
            <div className="mt-3">
              <Button size="sm" onClick={() => setShowInit(true)}>
                Initialize PKI
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Dialog open={showInit} onOpenChange={setShowInit}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Initialize PKI</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <p className="text-sm text-muted-foreground">
              This will create a Certificate Authority (CA) for your organization.
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
              {initPki.isPending ? 'Initializing...' : 'Initialize'}
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
