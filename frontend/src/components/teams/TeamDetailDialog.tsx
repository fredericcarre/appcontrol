import { useState } from 'react';
import { Team, TeamMember, useTeamMembers, useAddTeamMember, useRemoveTeamMember } from '@/api/teams';
import { UserSearchResult } from '@/api/users';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
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
import { Trash2, Users } from 'lucide-react';

interface TeamDetailDialogProps {
  team: Team | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function TeamDetailDialog({ team, open, onOpenChange }: TeamDetailDialogProps) {
  const { data: members, isLoading } = useTeamMembers(team?.id || '');
  const addMember = useAddTeamMember();
  const removeMember = useRemoveTeamMember();

  const [newMemberRole, setNewMemberRole] = useState('member');
  const [removingId, setRemovingId] = useState<string | null>(null);

  const handleAddMember = async (user: UserSearchResult) => {
    if (!team) return;
    await addMember.mutateAsync({
      team_id: team.id,
      user_id: user.id,
      role: newMemberRole,
    });
  };

  const handleRemoveMember = async (member: TeamMember) => {
    if (!team) return;
    setRemovingId(member.user_id);
    try {
      await removeMember.mutateAsync({
        team_id: team.id,
        user_id: member.user_id,
      });
    } finally {
      setRemovingId(null);
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

        <div className="space-y-4">
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

          <div className="border rounded-md">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Member</TableHead>
                  <TableHead>Email</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Joined</TableHead>
                  <TableHead className="w-[50px]"></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center py-8">
                      <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full mx-auto" />
                    </TableCell>
                  </TableRow>
                ) : !members?.length ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center text-muted-foreground py-8">
                      No members yet. Add someone above.
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
                      <TableCell>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8 text-destructive hover:text-destructive"
                          onClick={() => handleRemoveMember(member)}
                          disabled={removingId === member.user_id}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>

          <div className="text-xs text-muted-foreground">
            {members?.length || 0} member{(members?.length || 0) !== 1 ? 's' : ''}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
