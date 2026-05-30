import { List, useTable } from '@refinedev/antd';
import { Table, Tag } from 'antd';

export const BanksList = () => {
  const { tableProps } = useTable({ resource: 'banks' });

  return (
    <List>
      <Table {...tableProps} rowKey="id">
        <Table.Column dataIndex="routing_number" title="Routing #" />
        <Table.Column dataIndex="name" title="Name" />
        <Table.Column dataIndex="fedwire_name" title="FedWire Name" />
        <Table.Column
          dataIndex="fednow"
          title="FedNow"
          render={(v: boolean) =>
            v ? <Tag color="green">Yes</Tag> : <Tag>No</Tag>
          }
        />
        <Table.Column
          dataIndex="rtp"
          title="RTP"
          render={(v: boolean) =>
            v ? <Tag color="green">Yes</Tag> : <Tag>No</Tag>
          }
        />
        <Table.Column
          dataIndex="wire"
          title="Wire"
          render={(v: boolean) =>
            v ? <Tag color="green">Yes</Tag> : <Tag>No</Tag>
          }
        />
        <Table.Column dataIndex="ordering" title="Order" />
      </Table>
    </List>
  );
};
