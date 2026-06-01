/**
 * AuthContext — frontend-only mock authentication gate.
 *
 * ⚠️  FRONTEND MOCK — NOT REAL SECURITY ⚠️
 * This is a local unlock gate for a single-user self-hosted app.
 * Passwords are hashed with SHA-256 (SubtleCrypto) and stored in
 * localStorage.  The "unlocked" session is also persisted in localStorage.
 * This is NOT secure against someone with filesystem access to the device.
 *
 * Follow-up: migrate to a real self-hosted unlock (e.g. server-side session
 * validated by the Rust backend, or OS keychain integration).
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

// ── Storage keys ────────────────────────────────────────────────────────────
const KEY_HASH    = "pt-auth-hash";     // SHA-256 hex of master password
const KEY_SESSION = "pt-auth-session";  // "1" when unlocked for this session
const KEY_DEMO    = "pt-auth-demo";     // "1" when entered via demo shortcut

// ── SHA-256 helper (SubtleCrypto — browser native) ───────────────────────
async function sha256(input: string): Promise<string> {
  const buf = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(input),
  );
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

// ── Context shape ────────────────────────────────────────────────────────────
export interface AuthContextValue {
  /** True when the user has unlocked (or entered demo mode) this session */
  isUnlocked: boolean;
  /** True when a master password hash is stored (shows Login vs Setup) */
  hasPassword: boolean;
  /** True when the session was started via the demo shortcut */
  isDemo: boolean;
  /**
   * First-run setup: store the hashed password and mark session as unlocked.
   * Rejects if `pw` < 6 chars or `confirm` doesn't match.
   */
  setup: (pw: string, confirm: string) => Promise<{ ok: boolean; error?: string }>;
  /**
   * Unlock with an existing password.
   * Returns { ok: false, error } on wrong password.
   */
  unlock: (pw: string) => Promise<{ ok: boolean; error?: string }>;
  /** Lock the session (stays locked until next unlock/demo). */
  lock: () => void;
  /** Enter without a password — sets demo mode, skips password gate. */
  loginDemo: () => void;
  /**
   * "Lupa sandi?" — clears the stored hash so the user re-enters setup.
   * Also clears the current session.
   */
  resetPassword: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// ── Provider ─────────────────────────────────────────────────────────────────
export function AuthProvider({ children }: { children: ReactNode }) {
  const [hasPassword, setHasPassword] = useState<boolean>(
    () => Boolean(localStorage.getItem(KEY_HASH)),
  );
  const [isUnlocked, setIsUnlocked] = useState<boolean>(
    () =>
      localStorage.getItem(KEY_SESSION) === "1" ||
      localStorage.getItem(KEY_DEMO) === "1",
  );
  const [isDemo, setIsDemo] = useState<boolean>(
    () => localStorage.getItem(KEY_DEMO) === "1",
  );

  // Sync hasPassword if another tab clears the hash
  useEffect(() => {
    function onStorage(e: StorageEvent) {
      if (e.key === KEY_HASH) {
        setHasPassword(Boolean(e.newValue));
      }
    }
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const setup = useCallback(
    async (pw: string, confirm: string): Promise<{ ok: boolean; error?: string }> => {
      if (pw.length < 6) {
        return { ok: false, error: "Sandi minimal 6 karakter." };
      }
      if (pw !== confirm) {
        return { ok: false, error: "Konfirmasi sandi tidak cocok." };
      }
      const hash = await sha256(pw);
      localStorage.setItem(KEY_HASH, hash);
      localStorage.setItem(KEY_SESSION, "1");
      localStorage.removeItem(KEY_DEMO);
      setHasPassword(true);
      setIsUnlocked(true);
      setIsDemo(false);
      return { ok: true };
    },
    [],
  );

  const unlock = useCallback(
    async (pw: string): Promise<{ ok: boolean; error?: string }> => {
      const stored = localStorage.getItem(KEY_HASH);
      if (!stored) return { ok: false, error: "Tidak ada sandi tersimpan." };
      const hash = await sha256(pw);
      if (hash !== stored) {
        return { ok: false, error: "Sandi salah. Coba lagi." };
      }
      localStorage.setItem(KEY_SESSION, "1");
      localStorage.removeItem(KEY_DEMO);
      setIsUnlocked(true);
      setIsDemo(false);
      return { ok: true };
    },
    [],
  );

  const lock = useCallback(() => {
    localStorage.removeItem(KEY_SESSION);
    localStorage.removeItem(KEY_DEMO);
    setIsUnlocked(false);
    setIsDemo(false);
  }, []);

  const loginDemo = useCallback(() => {
    localStorage.setItem(KEY_DEMO, "1");
    localStorage.removeItem(KEY_SESSION);
    setIsUnlocked(true);
    setIsDemo(true);
  }, []);

  const resetPassword = useCallback(() => {
    localStorage.removeItem(KEY_HASH);
    localStorage.removeItem(KEY_SESSION);
    localStorage.removeItem(KEY_DEMO);
    setHasPassword(false);
    setIsUnlocked(false);
    setIsDemo(false);
  }, []);

  return (
    <AuthContext.Provider
      value={{ isUnlocked, hasPassword, isDemo, setup, unlock, lock, loginDemo, resetPassword }}
    >
      {children}
    </AuthContext.Provider>
  );
}

// ── Hook ─────────────────────────────────────────────────────────────────────
export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used inside <AuthProvider>");
  }
  return ctx;
}
