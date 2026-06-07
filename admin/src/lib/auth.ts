// Auth module for Depost Admin console.
// Stores the bearer token in localStorage under STORAGE_KEY.
// On boot, validates the token via GET /api/admin/me.
// Exposes a typed context and a useAuth() hook.

import { createContext, useContext, useEffect, useState, useCallback } from 'react';
import type { ReactNode } from 'react';
import { createElement } from 'react';

export const STORAGE_KEY = 'noor_admin_token';

export interface Operator {
  name: string;
  email: string;
  role: string;
}

export interface AuthState {
  /** null while booting (validating), undefined when unauthenticated, Operator when signed-in */
  operator: Operator | null | undefined;
  token: string | null;
  login: (email: string, password: string) => Promise<void>;
  logout: () => void;
}

// ---- bare fetch helpers (no circular dep with api.ts) -------------------------

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`/api/admin${path}`, init);
}

// ---- context -----------------------------------------------------------------

export const AuthContext = createContext<AuthState>({
  operator: undefined,
  token: null,
  login: async () => {},
  logout: () => {},
});

export function useAuth(): AuthState {
  return useContext(AuthContext);
}

// ---- provider ----------------------------------------------------------------

export function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(STORAGE_KEY));
  // null = "still checking"; undefined = "no session"; Operator = "signed in"
  const [operator, setOperator] = useState<Operator | null | undefined>(null);

  // Validate stored token on boot
  useEffect(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) {
      setOperator(undefined);
      return;
    }
    apiFetch('/me', {
      headers: { Authorization: `Bearer ${stored}` },
    })
      .then(async (res) => {
        if (res.ok) {
          const data: Operator = await res.json();
          setToken(stored);
          setOperator(data);
        } else {
          localStorage.removeItem(STORAGE_KEY);
          setToken(null);
          setOperator(undefined);
        }
      })
      .catch(() => {
        // Network error — clear session to be safe
        localStorage.removeItem(STORAGE_KEY);
        setToken(null);
        setOperator(undefined);
      });
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const res = await apiFetch('/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password }),
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(text || `Login failed (${res.status})`);
    }
    const data: { token: string; operator: Operator } = await res.json();
    localStorage.setItem(STORAGE_KEY, data.token);
    setToken(data.token);
    setOperator(data.operator);
  }, []);

  const logout = useCallback(() => {
    localStorage.removeItem(STORAGE_KEY);
    setToken(null);
    setOperator(undefined);
  }, []);

  return createElement(AuthContext.Provider, { value: { operator, token, login, logout } }, children);
}

// ---- 401 event ---------------------------------------------------------------
// api.ts dispatches this event when any request receives a 401.
// The AuthProvider listens and clears the session automatically.

export const AUTH_EXPIRED_EVENT = 'noor:auth:expired';

export function dispatchAuthExpired() {
  window.dispatchEvent(new Event(AUTH_EXPIRED_EVENT));
}
