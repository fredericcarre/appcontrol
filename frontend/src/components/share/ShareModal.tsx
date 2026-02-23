import { useState } from 'react';
import { useAppPermissions, useSetPermission, useRemovePermission, useShareLinks, useCreateShareLink, useRevokeShareLink } from '@/api/permissions';
import { UserSearchResult } from '@/api/users';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '@/components/ui/select';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Copy, Link, Plus, Trash2, Users, Clock } from 'lucide-react';
import { PERMISSION_LEVELS, permissionLabel } from '@/lib/permissions';
import { UserPicker } from './UserPicker';

interface ShareModalProps {
  appId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function ShareModal({ appId, open, onOpenChange }: ShareModalProps) {
  const { data: permissions } = useAppPermissions(appId);
  const { data: shareLinks } = useShareLinks(appId);
  const setPermission = useSetPermission();
  const removePermission = useRemovePermission();
  const createShareLink = useCreateShareLink();
  const revokeShareLink = useRevokeShareLink();

  const [newLevel, setNewLevel] = useState('view');
  const [linkLevel, setLinkLevel] = useState('view');
  const [copiedToken, setCopiedToken] = useState<string | null>(null);

  const handleAddUser = async (user: UserSearchResult) => {
    await setPermission.mutateAsync({ app_id: appId, user_id: user.id, level: newLevel });
  };

  const handleCreateLink = async () => {
    await createShareLink.mutateAsync({ app_id: appId, permission_level: linkLevel });
  };

  const copyLink = (token: string) => {
    navigator.clipboard.writeText(`${window.location.origin}/share/${token}`);
    setCopiedToken(token);
    setTimeout(() => setCopiedToken(null), 2000);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Share Application</DialogTitle>
        </DialogHeader>

        <Tabs defaultValue="users">
          <TabsList className="w-full">
            <TabsTrigger value="users" className="flex-1">
              <Users className="h-4 w-4 mr-1" /> Users & Teams
            </TabsTrigger>
            <TabsTrigger value="links" className="flex-1">
              <Link className="h-4 w-4 mr-1" /> Share Links
            </TabsTrigger>
          </TabsList>

          <TabsContent value="users" className="space-y-4">
            <div className="flex gap-2">
              <UserPicker onSelect={handleAddUser} />
              <Select value={newLevel} onValueChange={setNewLevel}>
                <SelectTrigger className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PERMISSION_LEVELS.filter((l) => l !== 'none').map((l) => (
                    <SelectItem key={l} value={l}>{permissionLabel(l)}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <Separator />

            <ScrollArea className="h-[240px]">
              <div className="space-y-2">
                {permissions?.map((p) => (
                  <div key={p.id} className="flex items-center justify-between p-2 rounded-md hover:bg-muted">
                    <div className="text-sm">
                      <span className="font-medium">{p.user_email || p.team_name || 'Unknown'}</span>
                      {p.team_id && <Badge variant="secondary" className="ml-2 text-xs">Team</Badge>}
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant="outline">{p.level}</Badge>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => removePermission.mutate({ app_id: appId, permission_id: p.id })}
                      >
                        <Trash2 className="h-3.5 w-3.5 text-destructive" />
                      </Button>
                    </div>
                  </div>
                ))}
                {!permissions?.length && (
                  <p className="text-sm text-muted-foreground text-center py-4">No permissions set</p>
                )}
              </div>
            </ScrollArea>
          </TabsContent>

          <TabsContent value="links" className="space-y-4">
            <div className="flex gap-2">
              <Select value={linkLevel} onValueChange={setLinkLevel}>
                <SelectTrigger className="flex-1">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PERMISSION_LEVELS.filter((l) => l !== 'none' && l !== 'owner').map((l) => (
                    <SelectItem key={l} value={l}>{permissionLabel(l)}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button onClick={handleCreateLink}>
                <Plus className="h-4 w-4 mr-1" /> Create Link
              </Button>
            </div>

            <Separator />

            <ScrollArea className="h-[240px]">
              <div className="space-y-2">
                {shareLinks?.map((link) => (
                  <div key={link.id} className="flex items-center justify-between p-2 rounded-md hover:bg-muted">
                    <div className="flex items-center gap-2 text-sm">
                      <Badge variant="outline">{link.permission_level}</Badge>
                      <span className="text-muted-foreground">
                        {link.current_uses}{link.max_uses ? `/${link.max_uses}` : ''} uses
                      </span>
                      {link.expires_at && (
                        <span className="text-xs text-muted-foreground flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {new Date(link.expires_at).toLocaleDateString()}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => copyLink(link.token)}
                      >
                        <Copy className={`h-3.5 w-3.5 ${copiedToken === link.token ? 'text-green-500' : ''}`} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => revokeShareLink.mutate({ app_id: appId, link_id: link.id })}
                      >
                        <Trash2 className="h-3.5 w-3.5 text-destructive" />
                      </Button>
                    </div>
                  </div>
                ))}
                {!shareLinks?.length && (
                  <p className="text-sm text-muted-foreground text-center py-4">No share links</p>
                )}
              </div>
            </ScrollArea>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
