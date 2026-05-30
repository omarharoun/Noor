import { List, Datagrid, TextField, BooleanField } from 'react-admin';

export const BankList = () => (
  <List perPage={50}>
    <Datagrid>
      <TextField source="id" label="Routing #" />
      <TextField source="name" label="Name" />
      <TextField source="routing_number" label="Routing Number" />
      <BooleanField source="supports_fednow" label="FedNow" />
      <BooleanField source="supports_rtp" label="RTP" />
      <BooleanField source="supports_wire" label="Wire" />
      <TextField source="display_order" label="Order" />
    </Datagrid>
  </List>
);
