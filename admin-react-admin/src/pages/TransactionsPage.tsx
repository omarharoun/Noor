import {
  List, Datagrid, TextField, NumberField, DateField, Show, SimpleShowLayout,
  TextInput, SelectInput, DateInput, FilterForm, type ListViewProps,
} from 'react-admin';
import { Box, Typography } from '@mui/material';

const sessionFilters = [
  <SelectInput key="status" source="status" label="Status" choices={[
    { id: 'pending', name: 'Pending' },
    { id: 'authorized', name: 'Authorized' },
    { id: 'processing', name: 'Processing' },
    { id: 'completed', name: 'Completed' },
    { id: 'expired', name: 'Expired' },
  ]} alwaysOn />,
  <SelectInput key="rail" source="rail" label="Rail" choices={[
    { id: 'fednow', name: 'FedNow' },
    { id: 'rtp', name: 'RTP' },
    { id: 'ach', name: 'ACH' },
    { id: 'wire', name: 'Wire' },
  ]} />,
  <DateInput key="date_from" source="date_from" label="From" />,
  <DateInput key="date_to" source="date_to" label="To" />,
];

const statusColors: Record<string, string> = {
  pending: '#ffa726',
  authorized: '#42a5f5',
  processing: '#7e57c2',
  completed: '#66bb6a',
  expired: '#ef5350',
};

export const SessionList = () => (
  <List
    perPage={50}
    filters={sessionFilters}
    sort={{ field: 'created_at', order: 'DESC' }}
  >
    <Datagrid
      rowClick="show"
      sx={{
        '& .column-status': {
          '& span': {
            px: 1, py: 0.5, borderRadius: 1, fontSize: '0.75rem', fontWeight: 700,
          },
        },
      }}
    >
      <TextField source="id" label="ID" />
      <NumberField source="amount_cents" label="Amount" options={{ style: 'currency', currency: 'USD' }}
        transform={(v: number) => v / 100} />
      <TextField source="status" label="Status" />
      <TextField source="rail_used" label="Rail" />
      <TextField source="merchant_id" label="Merchant" />
      <TextField source="customer_name" label="Customer" />
      <DateField source="created_at" label="Created" showTime />
    </Datagrid>
  </List>
);

export const SessionShow = () => (
  <Show>
    <SimpleShowLayout>
      <TextField source="id" label="ID" />
      <TextField source="merchant_id" label="Merchant ID" />
      <TextField source="bank_id" label="Bank ID" />
      <NumberField source="amount_cents" label="Amount"
        options={{ style: 'currency', currency: 'USD' }}
        transform={(v: number) => v / 100} />
      <TextField source="currency" label="Currency" />
      <TextField source="status" label="Status" />
      <TextField source="rail_used" label="Rail" />
      <TextField source="customer_name" label="Customer Name" />
      <TextField source="customer_email" label="Customer Email" />
      <TextField source="customer_phone" label="Customer Phone" />
      <TextField source="customer_account_number" label="Account Number" />
      <TextField source="customer_routing_number" label="Routing Number" />
      <TextField source="column_ref" label="Provider Ref" />
      <TextField source="column_counterparty_id" label="Counterparty ID" />
      <TextField source="note" label="Note" />
      <DateField source="created_at" label="Created" showTime />
      <DateField source="updated_at" label="Updated" showTime />
      <DateField source="expires_at" label="Expires" showTime />
    </SimpleShowLayout>
  </Show>
);
