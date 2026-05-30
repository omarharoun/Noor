import type { DataProvider } from 'react-admin';

const apiUrl = '/api/admin';

async function fetchJson(url: string, options?: RequestInit) {
  const res = await fetch(url, options);
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${res.statusText}`);
  return { json: await res.json() };
}

const httpClient = fetchJson;

export const dataProvider: DataProvider = {
  getList: async (resource, params) => {
    const { page = 1, perPage = 50 } = params.pagination || {};
    const { field, order } = params.sort || { field: 'created_at', order: 'DESC' };
    const query = new URLSearchParams({
      limit: String(perPage),
      offset: String((page - 1) * perPage),
    });

    const resourceMap: Record<string, string> = {
      sessions: 'sessions',
      merchants: 'merchants',
      settlements: 'settlements',
      webhooks: 'webhooks',
      banks: 'banks',
    };

    const endpoint = resourceMap[resource];
    if (!endpoint) throw new Error(`Unknown resource: ${resource}`);

    const url = `${apiUrl}/${endpoint}?${query}`;
    const { json } = await httpClient(url);

    if (resource === 'banks') {
      return { data: json.banks, total: json.banks.length };
    }
    if (resource === 'settlements') {
      return { data: json.settlements, total: json.settlements.length };
    }

    const dataKey = resource === 'sessions' ? 'sessions' :
                    resource === 'merchants' ? 'merchants' :
                    resource === 'webhooks' ? 'events' : resource;

    return {
      data: json[dataKey]?.map((item: any) => ({ ...item, id: item.id || item.merchant_id })) || [],
      total: json.total || 0,
    };
  },

  getOne: async (resource, params) => {
    if (resource === 'merchants') {
      const { json } = await httpClient(`${apiUrl}/merchants/${params.id}`);
      return { data: json };
    }
    if (resource === 'sessions') {
      const { json } = await httpClient(`${apiUrl}/sessions/${params.id}`);
      return { data: json };
    }
    throw new Error(`getOne not implemented for ${resource}`);
  },

  create: async (resource, params) => {
    if (resource === 'merchants') {
      const { json } = await httpClient(`${apiUrl}/merchants`, {
        method: 'POST',
        body: JSON.stringify(params.data),
      });
      return { data: json };
    }
    throw new Error(`create not implemented for ${resource}`);
  },

  update: async (resource, params) => {
    if (resource === 'merchants') {
      const { json } = await httpClient(`${apiUrl}/merchants/${params.id}`, {
        method: 'PUT',
        body: JSON.stringify(params.data),
      });
      return { data: json };
    }
    throw new Error(`update not implemented for ${resource}`);
  },

  getMany: async () => { throw new Error('getMany not implemented'); },
  getManyReference: async () => { throw new Error('getManyReference not implemented'); },
  updateMany: async () => { throw new Error('updateMany not implemented'); },
  delete: async () => { throw new Error('delete not implemented'); },
  deleteMany: async () => { throw new Error('deleteMany not implemented'); },
};
