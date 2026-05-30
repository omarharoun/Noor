import {
  List, Datagrid, TextField, NumberField, DateField,
} from 'react-admin';

export const WebhookList = () => (
  <List perPage={50} sort={{ field: 'created_at', order: 'DESC' }}>
    <Datagrid>
      <TextField source="event_type" label="Event Type" />
      <TextField source="status" label="Status" />
      <TextField source="merchant_id" label="Merchant" />
      <TextField source="session_id" label="Session" />
      <NumberField source="attempts" label="Attempts" />
      <DateField source="created_at" label="Received" showTime />
      <DateField source="sent_at" label="Sent" showTime />
    </Datagrid>
  </List>
);
