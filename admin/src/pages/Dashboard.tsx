import { useState, useEffect } from 'react';
import { Card, Col, Row, Statistic, Typography } from 'antd';
import {
  SwapOutlined,
  CheckCircleOutlined,
  ClockCircleOutlined,
  DollarOutlined,
} from '@ant-design/icons';
import {
  BarChart,
  Bar,
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';

interface Stats {
  today_total_transactions: number;
  today_completed: number;
  pending_count: number;
  total_volume_cents: number;
  by_rail: Array<{ rail_used: string; count: number; volume: number }>;
  last_7_days: Array<{ date: string; count: number; volume: number }>;
}

const { Title } = Typography;

export const Dashboard = () => {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch('/api/admin/stats')
      .then((r) => r.json())
      .then((data: Stats) => {
        setStats(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  if (loading || !stats) {
    return <Card loading />;
  }

  return (
    <div>
      <Title level={3}>Dashboard</Title>
      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="Today Total"
              value={stats.today_total_transactions}
              prefix={<SwapOutlined />}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="Completed"
              value={stats.today_completed}
              prefix={<CheckCircleOutlined />}
              valueStyle={{ color: '#3f8600' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="Pending"
              value={stats.pending_count}
              prefix={<ClockCircleOutlined />}
              valueStyle={{ color: '#faad14' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="Volume"
              value={(stats.total_volume_cents / 100).toFixed(2)}
              prefix={<DollarOutlined />}
              precision={2}
            />
          </Card>
        </Col>
      </Row>
      <Row gutter={[16, 16]} style={{ marginTop: 24 }}>
        <Col xs={24} lg={12}>
          <Card title="By Rail">
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={stats.by_rail}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="rail_used" />
                <YAxis />
                <Tooltip />
                <Bar dataKey="count" fill="#1677ff" name="Count" />
                <Bar dataKey="volume" fill="#52c41a" name="Volume" />
              </BarChart>
            </ResponsiveContainer>
          </Card>
        </Col>
        <Col xs={24} lg={12}>
          <Card title="Last 7 Days">
            <ResponsiveContainer width="100%" height={300}>
              <LineChart data={stats.last_7_days}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="date" />
                <YAxis />
                <Tooltip />
                <Line
                  type="monotone"
                  dataKey="count"
                  stroke="#1677ff"
                  name="Count"
                />
                <Line
                  type="monotone"
                  dataKey="volume"
                  stroke="#52c41a"
                  name="Volume"
                />
              </LineChart>
            </ResponsiveContainer>
          </Card>
        </Col>
      </Row>
    </div>
  );
};
