/**
 * AuthContext — server-backed auth gate.
 *
 * The master password is exchanged at POST /auth/login for a JWT, stored in
 * localStorage and attached to every API request by api/client.ts. A 401 from
 * any request dispatches `pt-unauthorized`, which locks the session.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { api } from "../api/client";
import { LoginResponseSchema } from "../api/schemas";

const TOKEN_KEY = "pt-auth-token";

export interface AuthContextValue {
  /** True when a token is stored for this session. */
  isUnlocked: boolean;
  /** Exchange the master password for a token. */
  unlock: (pw: string) => Promise<{ ok: boolean; error?: string }>;
  /** Clear the token and lock. */
  lock: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [isUnlocked, setIsUnlocked] = useState<boolean>(
    () => Boolean(localStorage.getItem(TOKEN_KEY)),
  );

  // Lock when any request reports the token is no longer valid.
  useEffect(() => {
    const onUnauthorized = () => setIsUnlocked(false);
    window.addEventListener("pt-unauthorized", onUnauthorized);
    return () => window.removeEventListener("pt-unauthorized", onUnauthorized);
  }, []);

  const unlock = useCallback(
    async (pw: string): Promise<{ ok: boolean; error?: string }> => {
      try {
        const { token } = await api.post("/auth/login", LoginResponseSchema, { password: pw });
        localStorage.setItem(TOKEN_KEY, token);
        setIsUnlocked(true);
        return { ok: true };
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }
    },
    [],
  );

  const lock = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    setIsUnlocked(false);
  }, []);

  return (
    <AuthContext.Provider value={{ isUnlocked, unlock, lock }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used inside <AuthProvider>");
  }
  return ctx;
}
