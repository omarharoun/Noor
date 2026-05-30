import type { DataProvider } from '@refinedev/core';
import simpleRestProvider from '@refinedev/simple-rest';

const API_URL = '/api/admin';

const resourceDataKeys: Record<string, string> = {
  sessions: 'sessions',
  merchants: 'merchants',
  banks: 'banks',
  settlements: 'settlements',
  webhooks: 'events',
};

const baseProvider = simpleRestProvider(API_URL);

export const dataProvider: DataProvider = {
  ...baseProvider,

  getList: async ({ resource, pagination, filters, sorters, meta }) => {
    const params = new URLSearchParams();

    const { current = 1, pageSize = 10 } = pagination ?? {};
    params.set('limit', String(pageSize));
    params.set('offset', String((current - 1) * pageSize));

    if (filters) {
      for (const filter of filters) {
        if ('field' in filter && filter.operator === 'eq' && filter.value !== undefined) {
          params.set(filter.field, String(filter.value));
        }
      }
    }

    if (sorters && sorters.length > 0) {
      params.set(
        'sort',
        sorters.map((s) => `${s.field}:${s.order}`).join(','),
      );
    }

    const response = await fetch(`${API_URL}/${resource}?${params}`);
    const json = await response.json();

    const key = resourceDataKeys[resource] || resource;
    const data: any[] = json[key] ?? [];
    const total: number = json.total ?? (Array.isArray(data) ? data.length : 0);

    return { data, total };
  },

  getOne: async ({ resource, id }) => {
    const response = await fetch(`${API_URL}/${resource}/${id}`);
    const json = await response.json();
    return { data: json };
  },

  create: async ({ resource, variables }) => {
    const response = await fetch(`${API_URL}/${resource}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(variables),
    });
    const json = await response.json();
    return { data: json };
  },

  update: async ({ resource, id, variables }) => {
    const response = await fetch(`${API_URL}/${resource}/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(variables),
    });
    const json = await response.json();
    return { data: json };
  },
};
