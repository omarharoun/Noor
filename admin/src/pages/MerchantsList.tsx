import { List, useTable, Show, Create, Edit, useForm } from '@refinedev/antd';
import { Table, Tag, Descriptions, Form, Input, Typography } from 'antd';
import { useShow } from '@refinedev/core';
import dayjs from 'dayjs';

export const MerchantsList = () => {
  const { tableProps } = useTable({
    resource: 'merchants',
    pagination: { pageSize: 20 },
  });

  return (
    <List>
      <Table {...tableProps} rowKey="id">
        <Table.Column dataIndex="id" title="ID" width={60} />
        <Table.Column dataIndex="name" title="Name" />
        <Table.Column dataIndex="email" title="Email" />
        <Table.Column
          dataIndex="status"
          title="Status"
          render={(v: string) => <Tag>{v}</Tag>}
        />
        <Table.Column
          dataIndex="kyc_status"
          title="KYC"
          render={(v: string) => (
            <Tag color={v === 'verified' ? 'green' : 'orange'}>{v}</Tag>
          )}
        />
        <Table.Column dataIndex="business_name" title="Business" />
        <Table.Column
          dataIndex="created_at"
          title="Created"
          render={(v: string) => dayjs(v).format('YYYY-MM-DD')}
        />
      </Table>
    </List>
  );
};

export const MerchantShow = () => {
  const { queryResult } = useShow({ resource: 'merchants' });
  const { data, isLoading } = queryResult;
  const record = data?.data;

  return (
    <Show isLoading={isLoading}>
      <Descriptions bordered column={2}>
        <Descriptions.Item label="ID">{record?.id}</Descriptions.Item>
        <Descriptions.Item label="Name">
          <Typography.Text>{record?.name}</Typography.Text>
        </Descriptions.Item>
        <Descriptions.Item label="Email">{record?.email}</Descriptions.Item>
        <Descriptions.Item label="Status">
          <Tag>{record?.status}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="KYC Status">
          <Tag
            color={record?.kyc_status === 'verified' ? 'green' : 'orange'}
          >
            {record?.kyc_status}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="Business Name">
          {record?.business_name}
        </Descriptions.Item>
        <Descriptions.Item label="Business Type">
          {record?.business_type}
        </Descriptions.Item>
        <Descriptions.Item label="Tax ID">
          {record?.tax_id}
        </Descriptions.Item>
        <Descriptions.Item label="Industry">
          {record?.industry_category}
        </Descriptions.Item>
        <Descriptions.Item label="Website">
          {record?.website_url}
        </Descriptions.Item>
        <Descriptions.Item label="Webhook URL">
          {record?.webhook_url}
        </Descriptions.Item>
        <Descriptions.Item label="Created">
          {record?.created_at
            ? dayjs(record.created_at).format('YYYY-MM-DD HH:mm')
            : ''}
        </Descriptions.Item>
      </Descriptions>
    </Show>
  );
};

export const MerchantCreate = () => {
  const { formProps, saveButtonProps } = useForm({
    resource: 'merchants',
    action: 'create',
  });

  return (
    <Create saveButtonProps={saveButtonProps}>
      <Form {...formProps} layout="vertical">
        <Form.Item
          label="Name"
          name="name"
          rules={[{ required: true, message: 'Please enter a name' }]}
        >
          <Input />
        </Form.Item>
        <Form.Item
          label="Email"
          name="email"
          rules={[
            { required: true, message: 'Please enter an email' },
            { type: 'email', message: 'Invalid email' },
          ]}
        >
          <Input />
        </Form.Item>
      </Form>
    </Create>
  );
};

export const MerchantEdit = () => {
  const { formProps, saveButtonProps, queryResult } = useForm({
    resource: 'merchants',
    action: 'edit',
  });

  return (
    <Edit
      saveButtonProps={saveButtonProps}
      isLoading={queryResult?.isLoading}
    >
      <Form {...formProps} layout="vertical">
        <Form.Item label="Business Name" name="business_name">
          <Input />
        </Form.Item>
        <Form.Item label="Business Type" name="business_type">
          <Input />
        </Form.Item>
        <Form.Item label="Tax ID" name="tax_id">
          <Input />
        </Form.Item>
        <Form.Item label="Industry Category" name="industry_category">
          <Input />
        </Form.Item>
        <Form.Item label="Website URL" name="website_url">
          <Input />
        </Form.Item>
        <Form.Item label="Webhook URL" name="webhook_url">
          <Input />
        </Form.Item>
      </Form>
    </Edit>
  );
};
