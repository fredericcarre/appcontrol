import { useState } from 'react';
import { useAddTeamMember } from '@/api/teams';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '@/components/ui/select';
import { UserPlus } from 'lucide-react';

interface InviteModalProps {
  teamId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function InviteModal({ teamId, open, onOpenChange }: InviteModalProps) {
  const addMember = useAddTeamMember();
  const [userId, setUserId] = useState('');
  const [role, setRole] = useState('member');

  const handleInvite = async () => {
    if (!userId.trim()) return;
    await addMember.mutateAsync({ team_id: teamId, user_id: userId.trim(), role });
    setUserId('');
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <UserPlus className="h-5 w-5" /> Invite Team Member
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">User ID or Email</label>
            <Input value={userId} onChange={(e) => setUserId(e.target.value)} placeholder="user@example.com" />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Role</label>
            <Select value={role} onValueChange={setRole}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="member">Member</SelectItem>
                <SelectItem value="admin">Admin</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={handleInvite} disabled={!userId.trim() || addMember.isPending}>
            {addMember.isPending ? 'Inviting...' : 'Invite'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
