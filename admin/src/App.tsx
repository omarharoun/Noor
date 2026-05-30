import { Refine } from '@refinedev/core';
import routerProvider, { RefineRoutes } from '@refinedev/react-router-v6';
import { useNotificationProvider, ThemedLayoutV2 } from '@refinedev/antd';
import '@refinedev/antd/dist/reset.css';
import { Routes, Route, Outlet } from 'react-router-dom';

import { dataProvider } from './dataProvider';
import { Dashboard } from './pages/Dashboard';
import { SessionsList } from './pages/SessionsList';
import {
  MerchantsList,
  MerchantShow,
  MerchantCreate,
  MerchantEdit,
} from './pages/MerchantsList';
import { BanksList } from './pages/BanksList';
import { SettlementsList } from './pages/SettlementsList';
import { WebhooksList } from './pages/WebhooksList';

function App() {
  return (
    <Refine
      routerProvider={routerProvider}
      dataProvider={dataProvider}
      notificationProvider={useNotificationProvider}
      DashboardPage={Dashboard}
      resources={[
        { name: 'sessions', list: SessionsList },
        {
          name: 'merchants',
          list: MerchantsList,
          show: MerchantShow,
          create: MerchantCreate,
          edit: MerchantEdit,
        },
        { name: 'banks', list: BanksList },
        { name: 'settlements', list: SettlementsList },
        { name: 'webhooks', list: WebhooksList },
      ]}
      options={{ syncWithLocation: true, warnWhenUnsavedChanges: true, disableTelemetry: true }}
    >
      <Routes>
        <Route element={<ThemedLayoutV2><Outlet /></ThemedLayoutV2>}>
          <Route index element={<Dashboard />} />
          <Route path="*" element={<RefineRoutes />} />
        </Route>
      </Routes>
    </Refine>
  );
}

export default App;
