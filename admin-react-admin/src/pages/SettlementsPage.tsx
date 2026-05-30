import {
  List, Datagrid, TextField, NumberField, DateField,
} from 'react-admin';

export const SettlementList = () => (
  <List perPage={50}>
    <Datagrid>
      <TextField source="merchant_name" label="Merchant" />
      <TextField source="merchant_id" label="Merchant ID" />
      <NumberField source="total_count" label="Payments" />
      <NumberField source="total_volume_cents" label="Total Volume"
        options={{ style: 'currency', currency: 'USD' }}
        transform={(v: number) => v / 100} />
      <DateField source="last_payment_date" label="Last Payment" showTime />
    </Datagrid>
  </List>
);
