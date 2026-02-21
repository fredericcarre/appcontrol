import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Team {
  id: string;
  name: string;
  description: string;
  org_id: string;
  member_count: number;
  created_at: string;
}

export interface TeamMember {
  user_id: string;
  email: string;
  name: string;
  role: string;
  joined_at: string;
}

export function useTeams() {
  return useQuery({
    queryKey: ['teams'],
    queryFn: async () => {
      const { data } = await client.get<Team[]>('/teams');
      return data;
    },
  });
}

export function useTeam(teamId: string) {
  return useQuery({
    queryKey: ['teams', teamId],
    queryFn: async () => {
      const { data } = await client.get<Team>(`/teams/${teamId}`);
      return data;
    },
    enabled: !!teamId,
  });
}

export function useTeamMembers(teamId: string) {
  return useQuery({
    queryKey: ['teams', teamId, 'members'],
    queryFn: async () => {
      const { data } = await client.get<TeamMember[]>(`/teams/${teamId}/members`);
      return data;
    },
    enabled: !!teamId,
  });
}

export function useCreateTeam() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { name: string; description: string }) => {
      const { data } = await client.post<Team>('/teams', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['teams'] }),
  });
}

export function useAddTeamMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { team_id: string; user_id: string; role: string }) => {
      await client.post(`/teams/${payload.team_id}/members`, payload);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['teams', vars.team_id, 'members'] }),
  });
}

export function useRemoveTeamMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { team_id: string; user_id: string }) => {
      await client.delete(`/teams/${payload.team_id}/members/${payload.user_id}`);
    },
    onSuccess: (_, vars) => qc.invalidateQueries({ queryKey: ['teams', vars.team_id, 'members'] }),
  });
}
