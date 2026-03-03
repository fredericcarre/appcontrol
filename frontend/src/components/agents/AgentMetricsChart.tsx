import { useAgentMetrics } from '@/api/agents';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useState } from 'react';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
} from 'recharts';

interface AgentMetricsChartProps {
  agentId: string;
  hostname: string;
}

export function AgentMetricsChart({ agentId, hostname }: AgentMetricsChartProps) {
  const [minutes, setMinutes] = useState(60);
  const { data, isLoading, error } = useAgentMetrics(agentId, minutes);

  // Transform data for recharts
  const chartData = data?.metrics.map((m) => ({
    time: new Date(m.created_at).toLocaleTimeString(),
    cpu: Math.round(m.cpu_pct * 10) / 10,
    memory: Math.round(m.memory_pct * 10) / 10,
    disk: m.disk_used_pct != null ? Math.round(m.disk_used_pct * 10) / 10 : null,
  })) || [];

  if (error) {
    return (
      <Card>
        <CardContent className="py-6 text-center text-muted-foreground">
          Failed to load metrics
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between pb-2">
        <CardTitle className="text-sm font-medium">
          Resource Usage - {hostname}
        </CardTitle>
        <Select value={String(minutes)} onValueChange={(v) => setMinutes(Number(v))}>
          <SelectTrigger className="w-[140px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="15">Last 15 min</SelectItem>
            <SelectItem value="60">Last hour</SelectItem>
            <SelectItem value="360">Last 6 hours</SelectItem>
            <SelectItem value="1440">Last 24 hours</SelectItem>
          </SelectContent>
        </Select>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="h-[200px] flex items-center justify-center">
            <div className="animate-spin h-6 w-6 border-2 border-primary border-t-transparent rounded-full" />
          </div>
        ) : chartData.length === 0 ? (
          <div className="h-[200px] flex items-center justify-center text-muted-foreground">
            No metrics data available
          </div>
        ) : (
          <ResponsiveContainer width="100%" height={200}>
            <LineChart data={chartData}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="time"
                tick={{ fontSize: 11 }}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                domain={[0, 100]}
                tick={{ fontSize: 11 }}
                tickLine={false}
                axisLine={false}
                tickFormatter={(v) => `${v}%`}
              />
              <Tooltip
                contentStyle={{
                  backgroundColor: 'hsl(var(--popover))',
                  border: '1px solid hsl(var(--border))',
                  borderRadius: '6px',
                }}
                labelStyle={{ color: 'hsl(var(--foreground))' }}
              />
              <Legend />
              <Line
                type="monotone"
                dataKey="cpu"
                name="CPU"
                stroke="#2563eb"
                strokeWidth={2}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="memory"
                name="Memory"
                stroke="#16a34a"
                strokeWidth={2}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="disk"
                name="Disk"
                stroke="#ea580c"
                strokeWidth={2}
                dot={false}
                connectNulls
              />
            </LineChart>
          </ResponsiveContainer>
        )}
      </CardContent>
    </Card>
  );
}
