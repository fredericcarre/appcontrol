import { useTeams } from '@/api/teams';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Users } from 'lucide-react';

interface TeamListProps {
  onSelect: (teamId: string) => void;
}

export function TeamList({ onSelect }: TeamListProps) {
  const { data: teams, isLoading } = useTeams();

  if (isLoading) {
    return <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full mx-auto" />;
  }

  return (
    <div className="space-y-2">
      {teams?.map((team) => (
        <button
          key={team.id}
          onClick={() => onSelect(team.id)}
          className="w-full text-left"
        >
          <Card className="hover:bg-accent transition-colors">
            <CardContent className="p-3 flex items-center gap-3">
              <Users className="h-5 w-5 text-muted-foreground" />
              <div className="flex-1">
                <p className="font-medium text-sm">{team.name}</p>
                <p className="text-xs text-muted-foreground">{team.description}</p>
              </div>
              <Badge variant="secondary">{team.member_count}</Badge>
            </CardContent>
          </Card>
        </button>
      ))}
    </div>
  );
}
