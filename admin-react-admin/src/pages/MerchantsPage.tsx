import {
  List, Datagrid, TextField, DateField, Show, SimpleShowLayout,
  Create, SimpleForm, TextInput, Edit, EditButton,
} from 'react-admin';

export const MerchantList = () => (
  <List perPage={50} sort={{ field: 'created_at', order: 'DESC' }}>
    <Datagrid rowClick="show">
      <TextField source="id" label="ID" />
      <TextField source="name" label="Name" />
      <TextField source="email" label="Email" />
      <TextField source="status" label="Status" />
      <TextField source="kyc_status" label="KYC" />
      <TextField source="business_name" label="Business" />
      <DateField source="created_at" label="Created" showTime />
      <EditButton />
    </Datagrid>
  </List>
);

export const MerchantShow = () => (
  <Show>
    <SimpleShowLayout>
      <TextField source="id" label="ID" />
      <TextField source="name" label="Name" />
      <TextField source="email" label="Email" />
      <TextField source="api_key" label="API Key" />
      <TextField source="status" label="Status" />
      <TextField source="kyc_status" label="KYC Status" />
      <TextField source="risk_level" label="Risk Level" />
      <TextField source="business_name" label="Business Name" />
      <TextField source="business_type" label="Business Type" />
      <TextField source="registration_number" label="Registration #" />
      <TextField source="tax_id" label="Tax ID" />
      <TextField source="industry_category" label="Industry" />
      <TextField source="website_url" label="Website" />
      <TextField source="webhook_url" label="Webhook URL" />
      <DateField source="onboarding_completed_at" label="Onboarding Completed" showTime />
      <DateField source="created_at" label="Created" showTime />
      <DateField source="updated_at" label="Updated" showTime />
    </SimpleShowLayout>
  </Show>
);

export const MerchantCreate = () => (
  <Create>
    <SimpleForm>
      <TextInput source="name" label="Name" fullWidth required />
      <TextInput source="email" label="Email" fullWidth required />
    </SimpleForm>
  </Create>
);

export const MerchantEdit = () => (
  <Edit>
    <SimpleForm>
      <TextInput source="business_name" label="Business Name" fullWidth />
      <TextInput source="business_type" label="Business Type" fullWidth />
      <TextInput source="tax_id" label="Tax ID" fullWidth />
      <TextInput source="industry_category" label="Industry" fullWidth />
      <TextInput source="website_url" label="Website URL" fullWidth />
      <TextInput source="webhook_url" label="Webhook URL" fullWidth />
    </SimpleForm>
  </Edit>
);
