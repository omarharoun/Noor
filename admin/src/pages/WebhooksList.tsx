import { List, useTable } from '@refinedev/antd';
import { Table, Tag } from 'antd';
import dayjs from 'dayjs';

const statusColor: Record<string, string> = {
  delivered: 'green',
  failed: 'red',
  retrying: 'orange',
  pending: 'gold',
};

export const WebhooksList = () => {
  const { tableProps } = useTable({ resource: 'webhooks' });

  return (
    <List>
      <Table {...tableProps} rowKey="id">
        <Table.Column dataIndex="event_type" title="Event Type" />
        <Table.Column
          dataIndex="status"
          title="Status"
          render={(v: string) => <Tag color={statusColor[v]}>{v}</Tag>}
        />
        <Table.Column dataIndex="merchant_name" title="Merchant" />
        <Table.Column dataIndex="session_id" title="Session ID" />
        <Table.Column dataIndex="attempts" title="Attempts" />
        <Table.Column
          dataIndex="created_at"
          title="Received"
          render={(v: string) => dayjs(v).format('YYYY-MM-DD HH:mm')}
        />
      </Table>
    </List>
  );
};
