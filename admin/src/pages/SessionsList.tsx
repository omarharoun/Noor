import { List, useTable } from '@refinedev/antd';
import { Table, Tag, DatePicker, Select, Button, Form, Space } from 'antd';
import dayjs from 'dayjs';

const statusColors: Record<string, string> = {
  completed: 'green',
  pending: 'gold',
  failed: 'red',
  processing: 'blue',
};

export const SessionsList = () => {
  const { tableProps, setFilters } = useTable({
    resource: 'sessions',
    pagination: { pageSize: 20 },
  });

  const [form] = Form.useForm();

  const handleFilter = (values: {
    status?: string;
    rail?: string;
    date_from?: dayjs.Dayjs;
    date_to?: dayjs.Dayjs;
  }) => {
    const f: Array<{ field: string; operator: 'eq'; value: string }> = [];
    if (values.status) f.push({ field: 'status', operator: 'eq', value: values.status });
    if (values.date_from) f.push({ field: 'date_from', operator: 'eq', value: values.date_from.format('YYYY-MM-DD') });
    if (values.date_to) f.push({ field: 'date_to', operator: 'eq', value: values.date_to.format('YYYY-MM-DD') });
    setFilters(f);
  };

  const handleReset = () => {
    form.resetFields();
    setFilters([]);
  };

  return (
    <List>
      <Form form={form} layout="inline" onFinish={handleFilter} style={{ marginBottom: 16 }}>
        <Form.Item name="status">
          <Select placeholder="Status" allowClear style={{ width: 130 }}>
            <Select.Option value="pending">Pending</Select.Option>
            <Select.Option value="completed">Completed</Select.Option>
            <Select.Option value="failed">Failed</Select.Option>
            <Select.Option value="processing">Processing</Select.Option>
          </Select>
        </Form.Item>
        <Form.Item name="date_from">
          <DatePicker placeholder="From" />
        </Form.Item>
        <Form.Item name="date_to">
          <DatePicker placeholder="To" />
        </Form.Item>
        <Form.Item>
          <Space>
            <Button type="primary" htmlType="submit">
              Filter
            </Button>
            <Button onClick={handleReset}>Reset</Button>
          </Space>
        </Form.Item>
      </Form>
      <Table
        {...tableProps}
        rowKey="id"
        expandable={{
          expandedRowRender: (record: Record<string, unknown>) => (
            <pre style={{ margin: 0 }}>{JSON.stringify(record, null, 2)}</pre>
          ),
        }}
      >
        <Table.Column dataIndex="id" title="ID" width={80} />
        <Table.Column
          dataIndex="amount_cents"
          title="Amount"
          render={(v: number) => `$${(v / 100).toFixed(2)}`}
        />
        <Table.Column
          dataIndex="status"
          title="Status"
          render={(v: string) => <Tag color={statusColors[v]}>{v}</Tag>}
        />
        <Table.Column dataIndex="rail" title="Rail" />
        <Table.Column dataIndex="customer_name" title="Customer" />
        <Table.Column
          dataIndex="created_at"
          title="Created"
          render={(v: string) => dayjs(v).format('YYYY-MM-DD HH:mm')}
        />
      </Table>
    </List>
  );
};
