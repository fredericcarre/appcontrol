import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Team, TeamMember, useTeamMembers, useTeamApps, useAddTeamMember, useRemoveTeamMember } from '@/api/teams';
import { UserSearchResult } from '@/api/users';
import { useAuthStore } from '@/stores/auth';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { UserPicker } from '@/components/share/UserPicker';
import { Trash2, Users, UserMinus, LayoutGrid } from 'lucide-react';
import { permissionLabel, PermissionLevel } from '@/lib/permissions';

interface TeamDetailDialogProps {
  team: Team | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function TeamDetailDialog({ team, open, onOpenChange }: TeamDetailDialogProps) {
  const currentUser = useAuthStore((s) => s.user);
  const navigate = useNavigate();
  const { data: members, isLoading } = useTeamMembers(team?.id || '');
  const { data: teamApps, isLoading: appsLoading } = useTeamApps(team?.id || '');
  const addMember = useAddTeamMember();
  const removeMember = useRemoveTeamMember();

  const [newMemberRole, setNewMemberRole] = useState('member');
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [removeConfirm, setRemoveConfirm] = useState<TeamMember | null>(null);

  // Check if current user can manage this team (admin or team lead)
  const isAdmin = currentUser?.role === 'admin';
  const isTeamLead = members?.some(
    (m) => m.user_id === currentUser?.id && m.role === 'lead'
  );
  const canManageTeam = isAdmin || isTeamLead;

  const handleAddMember = async (user: UserSearchResult) => {
    if (!team) return;
    await addMember.mutateAsync({
      team_id: team.id,
      user_id: user.id,
      role: newMemberRole,
    });
  };

  const handleRemoveMember = async () => {
    if (!team || !removeConfirm) return;
    setRemovingId(removeConfirm.user_id);
    try {
      await removeMember.mutateAsync({
        team_id: team.id,
        user_id: removeConfirm.user_id,
      });
    } finally {
      setRemovingId(null);
      setRemoveConfirm(null);
    }
  };

  const getInitials = (name: string) =>
    name
      .split(' ')
      .map((n) => n[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            {team?.name || 'Team'}
          </DialogTitle>
          {team?.description && (
            <DialogDescription>{team.description}</DialogDescription>
          )}
        </DialogHeader>

        <Tabs defaultValue="members">
          <TabsList className="w-full">
            <TabsTrigger value="members" className="flex-1">
              <Users className="h-4 w-4 mr-1" />
              Members ({members?.length || 0})
            </TabsTrigger>
            <TabsTrigger value="apps" className="flex-1">
              <LayoutGrid className="h-4 w-4 mr-1" />
              Applications ({teamApps?.length || 0})
            </TabsTrigger>
          </TabsList>

          <TabsContent value="members" className="space-y-4">
            {canManageTeam && (
              <div className="flex gap-2">
                <UserPicker onSelect={handleAddMember} placeholder="Add a team member..." />
                <Select value={newMemberRole} onValueChange={setNewMemberRole}>
                  <SelectTrigger className="w-28">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="lead">Lead</SelectItem>
                    <SelectItem value="member">Member</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            )}

            <div className="border rounded-md">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Member</TableHead>
                    <TableHead>Email</TableHead>
                    <TableHead>Role</TableHead>
                    <TableHead>Joined</TableHead>
                    {canManageTeam && <TableHead className="w-[50px]"></TableHead>}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    <TableRow>
                      <TableCell colSpan={canManageTeam ? 5 : 4} className="text-center py-8">
                        <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full mx-auto" />
                      </TableCell>
                    </TableRow>
                  ) : !members?.length ? (
                    <TableRow>
                      <TableCell colSpan={canManageTeam ? 5 : 4} className="text-center text-muted-foreground py-8">
                        {canManageTeam ? 'No members yet. Add someone above.' : 'No members yet.'}
                      </TableCell>
                    </TableRow>
                  ) : (
                    members.map((member) => (
                      <TableRow key={member.id}>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <Avatar className="h-8 w-8">
                              <AvatarFallback className="text-xs">
                                {getInitials(member.name)}
                              </AvatarFallback>
                            </Avatar>
                            <span className="font-medium">{member.name}</span>
                          </div>
                        </TableCell>
                        <TableCell className="text-muted-foreground">
                          {member.email}
                        </TableCell>
                        <TableCell>
                          <Badge variant={member.role === 'lead' ? 'default' : 'secondary'}>
                            {member.role}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-muted-foreground text-sm">
                          {new Date(member.joined_at).toLocaleDateString()}
                        </TableCell>
                        {canManageTeam && (
                          <TableCell>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-8 w-8 text-destructive hover:text-destructive"
                              onClick={() => setRemoveConfirm(member)}
                              disabled={removingId === member.user_id}
                            >
                              <Trash2 className="h-4 w-4" />
                            </Button>
                          </TableCell>
                        )}
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </div>
          </TabsContent>

          <TabsContent value="apps" className="space-y-4">
            <div className="border rounded-md">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Application</TableHead>
                    <TableHead>Description</TableHead>
                    <TableHead>Permission</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {appsLoading ? (
                    <TableRow>
                      <TableCell colSpan={3} className="text-center py-8">
                        <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full mx-auto" />
                      </TableCell>
                    </TableRow>
                  ) : !teamApps?.length ? (
                    <TableRow>
                      <TableCell colSpan={3} className="text-center text-muted-foreground py-8">
                        No applications shared with this team yet.
                        {canManageTeam && ' Use the Share button on an application to grant access.'}
                      </TableCell>
                    </TableRow>
                  ) : (
                    teamApps.map((app) => (
                      <TableRow
                        key={app.id}
                        className="cursor-pointer hover:bg-muted/50"
                        onClick={() => { onOpenChange(false); navigate(`/apps/${app.id}`); }}
                      >
                        <TableCell className="font-medium">{app.name}</TableCell>
                        <TableCell className="text-muted-foreground text-sm">
                          {app.description || '-'}
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline">{permissionLabel(app.permission_level as PermissionLevel)}</Badge>
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>

      {/* Remove Member Confirmation Dialog */}
      <Dialog open={!!removeConfirm} onOpenChange={(open) => !open && setRemoveConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <UserMinus className="h-5 w-5 text-destructive" />
              Remove Team Member
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to remove{' '}
              <span className="font-medium">{removeConfirm?.name || removeConfirm?.email}</span>{' '}
              from this team? They will lose access to team resources.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRemoveConfirm(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleRemoveMember}
              disabled={removeMember.isPending}
            >
              {removeMember.isPending ? 'Removing...' : 'Remove'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Dialog>
  );
}
