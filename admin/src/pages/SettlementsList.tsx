import { List, useTable } from '@refinedev/antd';
import { Table } from 'antd';
import dayjs from 'dayjs';

export const SettlementsList = () => {
  const { tableProps } = useTable({ resource: 'settlements' });

  return (
    <List>
      <Table {...tableProps} rowKey="id">
        <Table.Column dataIndex="id" title="ID" />
        <Table.Column dataIndex="merchant_name" title="Merchant" />
        <Table.Column dataIndex="payment_count" title="Payments" />
        <Table.Column
          dataIndex="total_volume_cents"
          title="Volume"
          render={(v: number) => `$${(v / 100).toFixed(2)}`}
        />
        <Table.Column
          dataIndex="last_payment_at"
          title="Last Payment"
          render={(v: string) =>
            v ? dayjs(v).format('YYYY-MM-DD') : '-'
          }
        />
      </Table>
    </List>
  );
};
