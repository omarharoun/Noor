import { Admin, Resource, ListGuesser } from 'react-admin';
import { dataProvider } from './dataProvider';
import { darkTheme } from './themes/darkTheme';
import Dashboard from './pages/Dashboard';
import { SessionList, SessionShow } from './pages/TransactionsPage';
import {
  MerchantList, MerchantShow, MerchantCreate, MerchantEdit,
} from './pages/MerchantsPage';
import { BankList } from './pages/BanksPage';
import { SettlementList } from './pages/SettlementsPage';
import { WebhookList } from './pages/WebhooksPage';
import PaymentIcon from '@mui/icons-material/Payment';
import StoreIcon from '@mui/icons-material/Store';
import AccountBalanceIcon from '@mui/icons-material/AccountBalance';
import ReceiptIcon from '@mui/icons-material/Receipt';
import WebhookIcon from '@mui/icons-material/Webhook';

export default function App() {
  return (
    <Admin
      dashboard={Dashboard}
      dataProvider={dataProvider}
      theme={darkTheme}
      requireAuth={false}
    >
      <Resource
        name="sessions"
        options={{ label: 'Transactions' }}
        list={SessionList}
        show={SessionShow}
        icon={PaymentIcon}
      />
      <Resource
        name="merchants"
        options={{ label: 'Merchants' }}
        list={MerchantList}
        show={MerchantShow}
        create={MerchantCreate}
        edit={MerchantEdit}
        icon={StoreIcon}
      />
      <Resource
        name="banks"
        options={{ label: 'Banks' }}
        list={BankList}
        icon={AccountBalanceIcon}
      />
      <Resource
        name="settlements"
        options={{ label: 'Settlements' }}
        list={SettlementList}
        icon={ReceiptIcon}
      />
      <Resource
        name="webhooks"
        options={{ label: 'Webhook Log' }}
        list={WebhookList}
        icon={WebhookIcon}
      />
    </Admin>
  );
}
