import { useEffect, useState } from 'react';
import { Card, CardContent, Typography, Grid2, Box } from '@mui/material';
import {
  TrendingUp, PendingActions, CheckCircle, AttachMoney,
} from '@mui/icons-material';
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  LineChart, Line, Legend,
} from 'recharts';

interface Stats {
  today_total_transactions: number;
  today_completed: number;
  pending_count: number;
  total_volume_cents: number;
  by_rail: { rail_used: string | null; count: number; volume: number }[];
  last_7_days: { date: string; count: number; volume: number }[];
}

const statCards = [
  { label: 'Today Total', key: 'today_total_transactions', icon: TrendingUp, color: '#90caf9' },
  { label: 'Today Completed', key: 'today_completed', icon: CheckCircle, color: '#66bb6a' },
  { label: 'Pending Payments', key: 'pending_count', icon: PendingActions, color: '#ffa726' },
  { label: 'Total Volume', key: 'total_volume_cents', icon: AttachMoney, color: '#ab47bc', format: (v: number) => `$${(v / 100).toLocaleString()}` },
];

export default function Dashboard() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch('/api/admin/stats')
      .then((r) => r.json())
      .then((data) => setStats(data))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <Typography sx={{ p: 3, color: 'text.secondary' }}>Loading dashboard...</Typography>;
  if (!stats) return <Typography sx={{ p: 3, color: 'error.main' }}>Failed to load stats</Typography>;

  const railData = (stats.by_rail || []).map((r) => ({
    name: (r.rail_used || 'unknown').toUpperCase(),
    count: Number(r.count),
    volume: Number(r.volume) / 100,
  }));

  const dailyData = (stats.last_7_days || []).map((d) => ({
    date: d.date,
    count: Number(d.count),
    volume: Number(d.volume) / 100,
  }));

  return (
    <Box sx={{ p: 3 }}>
      <Typography variant="h4" sx={{ mb: 3, fontWeight: 700, color: '#e3f2fd' }}>
        Payment Operations Dashboard
      </Typography>
      <Grid2 container spacing={3} sx={{ mb: 4 }}>
        {statCards.map(({ label, key, icon: Icon, color, format }) => (
          <Grid2 key={key} size={{ xs: 12, sm: 6, md: 3 }}>
            <Card>
              <CardContent>
                <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <Box>
                    <Typography variant="overline" sx={{ color: 'text.secondary' }}>{label}</Typography>
                    <Typography variant="h4" sx={{ fontWeight: 700, color }}>
                      {format ? format((stats as any)[key]) : Number((stats as any)[key]).toLocaleString()}
                    </Typography>
                  </Box>
                  <Icon sx={{ fontSize: 48, opacity: 0.3, color }} />
                </Box>
              </CardContent>
            </Card>
          </Grid2>
        ))}
      </Grid2>

      <Grid2 container spacing={3}>
        <Grid2 size={{ xs: 12, md: 6 }}>
          <Card>
            <CardContent>
              <Typography variant="h6" sx={{ mb: 2, color: '#e3f2fd' }}>Payments by Rail</Typography>
              <ResponsiveContainer width="100%" height={300}>
                <BarChart data={railData}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#1e4976" />
                  <XAxis dataKey="name" stroke="#b2bac2" />
                  <YAxis stroke="#b2bac2" />
                  <Tooltip
                    contentStyle={{ background: '#132f4c', border: '1px solid #1e4976', borderRadius: 8 }}
                    labelStyle={{ color: '#e3f2fd' }}
                  />
                  <Bar dataKey="count" fill="#90caf9" name="Count" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </CardContent>
          </Card>
        </Grid2>
        <Grid2 size={{ xs: 12, md: 6 }}>
          <Card>
            <CardContent>
              <Typography variant="h6" sx={{ mb: 2, color: '#e3f2fd' }}>Volume Last 7 Days</Typography>
              <ResponsiveContainer width="100%" height={300}>
                <LineChart data={dailyData}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#1e4976" />
                  <XAxis dataKey="date" stroke="#b2bac2" />
                  <YAxis stroke="#b2bac2" />
                  <Tooltip
                    contentStyle={{ background: '#132f4c', border: '1px solid #1e4976', borderRadius: 8 }}
                    labelStyle={{ color: '#e3f2fd' }}
                  />
                  <Legend />
                  <Line type="monotone" dataKey="volume" stroke="#66bb6a" name="Volume ($)" strokeWidth={2} dot={{ fill: '#66bb6a' }} />
                  <Line type="monotone" dataKey="count" stroke="#90caf9" name="Count" strokeWidth={2} dot={{ fill: '#90caf9' }} />
                </LineChart>
              </ResponsiveContainer>
            </CardContent>
          </Card>
        </Grid2>
      </Grid2>
    </Box>
  );
}
