import { useState } from 'react';
import { useApiKeys, useCreateApiKey, useDeleteApiKey } from '@/api/apiKeys';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Key, Plus, Trash2, Copy, CheckCircle, AlertTriangle } from 'lucide-react';

export function ApiKeysPage() {
  const { data: keys, isLoading } = useApiKeys();
  const createKey = useCreateApiKey();
  const deleteKey = useDeleteApiKey();
  const [showCreate, setShowCreate] = useState(false);
  const [newKeyName, setNewKeyName] = useState('');
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleCreate = async () => {
    if (!newKeyName.trim()) return;
    const result = await createKey.mutateAsync({ name: newKeyName.trim() });
    setCreatedKey(result.key);
    setNewKeyName('');
  };

  const handleCopyKey = () => {
    if (createdKey) {
      navigator.clipboard.writeText(createdKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleCloseCreate = () => {
    setShowCreate(false);
    setCreatedKey(null);
    setNewKeyName('');
  };

  return (
    <div className="space-y-6 max-w-3xl">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">API Keys</h1>
        <Button onClick={() => setShowCreate(true)}>
          <Plus className="h-4 w-4 mr-1" /> Create API Key
        </Button>
      </div>

      <p className="text-sm text-muted-foreground">
        API keys allow external tools (schedulers, CI/CD, scripts) to authenticate with AppControl's REST API.
        Keys use the <code className="bg-muted px-1 rounded">Authorization: Bearer ac_...</code> header.
      </p>

      {isLoading ? (
        <p className="text-muted-foreground">Loading...</p>
      ) : !keys?.length ? (
        <Card>
          <CardContent className="py-8 text-center">
            <Key className="h-10 w-10 text-muted-foreground mx-auto mb-3" />
            <p className="text-muted-foreground">No API keys yet. Create one to get started.</p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-3">
          {keys.map((key) => (
            <Card key={key.id}>
              <CardContent className="flex items-center justify-between py-4">
                <div className="flex items-center gap-3">
                  <Key className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <p className="font-medium">{key.name}</p>
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                      <code>{key.key_prefix}...</code>
                      <Badge variant={key.is_active ? 'default' : 'secondary'}>
                        {key.is_active ? 'Active' : 'Revoked'}
                      </Badge>
                      {key.expires_at && (
                        <span>Expires {new Date(key.expires_at).toLocaleDateString()}</span>
                      )}
                      <span>Created {new Date(key.created_at).toLocaleDateString()}</span>
                    </div>
                  </div>
                </div>
                {key.is_active && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="text-destructive"
                    onClick={() => deleteKey.mutate(key.id)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={showCreate} onOpenChange={handleCloseCreate}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{createdKey ? 'API Key Created' : 'Create API Key'}</DialogTitle>
          </DialogHeader>

          {createdKey ? (
            <div className="space-y-4">
              <div className="flex items-start gap-2 p-3 rounded-md bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800">
                <AlertTriangle className="h-5 w-5 text-amber-600 shrink-0 mt-0.5" />
                <p className="text-sm text-amber-800 dark:text-amber-200">
                  Copy this key now. You won't be able to see it again.
                </p>
              </div>
              <div className="flex gap-2">
                <Input value={createdKey} readOnly className="font-mono text-xs" />
                <Button variant="outline" size="icon" onClick={handleCopyKey}>
                  {copied ? <CheckCircle className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
                </Button>
              </div>
              <DialogFooter>
                <Button onClick={handleCloseCreate}>Done</Button>
              </DialogFooter>
            </div>
          ) : (
            <div className="space-y-4">
              <Input
                placeholder="Key name (e.g. Control-M integration)"
                value={newKeyName}
                onChange={(e) => setNewKeyName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
              />
              <DialogFooter>
                <Button variant="outline" onClick={handleCloseCreate}>Cancel</Button>
                <Button onClick={handleCreate} disabled={!newKeyName.trim() || createKey.isPending}>
                  Create
                </Button>
              </DialogFooter>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
