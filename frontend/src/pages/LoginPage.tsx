/**
 * LoginPage — Phase 5E auth screen.
 *
 * ⚠️  FRONTEND MOCK — NOT REAL SECURITY ⚠️
 * This is a local unlock gate for a single-user self-hosted app.
 * See AuthContext.tsx for details. Not ported to a real server-side
 * session yet — that is a follow-up task.
 *
 * Layout: split left/right on desktop (left brand aside hidden on mobile).
 *
 * States:
 *   - First-run setup  (hasPassword === false) — "Buat sandi master" + confirm
 *   - Login            (hasPassword === true)  — unlock field + "Lupa sandi?"
 *
 * Features: show/hide password affix, shake on wrong password, demo shortcut,
 * theme toggle in corner, © catalystlabs.id footer.
 */

import { useState, useRef, type FormEvent } from "react";
import { Eye, EyeOff, PieChart, TrendingUp, Shield, Zap, Sun, Moon, AlertCircle } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { useAuth } from "@/auth/AuthContext";

// ── Brand aside features ────────────────────────────────────────────────────

const FEATURES = [
  {
    icon: TrendingUp,
    title: "Lacak net worth real-time",
    desc: "Semua aset dalam satu tampilan — saham, crypto, reksa dana, kas.",
  },
  {
    icon: Shield,
    title: "Data tersimpan lokal",
    desc: "Self-hosted, data di tangan Anda. Tidak ada pihak ketiga.",
  },
  {
    icon: Zap,
    title: "Analisis dengan AI",
    desc: "Tanya portofolio dalam bahasa natural, dapat insight langsung.",
  },
];

// ── Small sub-components ────────────────────────────────────────────────────

function BrandAside() {
  return (
    <aside className="auth-aside">
      <div className="auth-aside-top">
        <div className="auth-aside-brand">
          <div className="auth-aside-mark">
            <PieChart size={20} strokeWidth={2.2} />
          </div>
          <span className="auth-aside-name">Portfolio</span>
        </div>
        <p className="auth-aside-tagline">
          Kelola keuangan<br />lebih cerdas, lebih tenang.
        </p>
        <div className="auth-aside-features">
          {FEATURES.map((f) => (
            <div key={f.title} className="auth-aside-feature">
              <div className="auth-aside-feature-icon">
                <f.icon size={15} strokeWidth={2} />
              </div>
              <div>
                <div style={{ fontWeight: 600, marginBottom: 2 }}>{f.title}</div>
                <div>{f.desc}</div>
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="auth-aside-bottom">© 2026 catalystlabs.id</div>
    </aside>
  );
}

function ThemeCornerBtn() {
  const { theme, setTheme } = useTheme();
  const isDark = theme === "dark";
  return (
    <button
      type="button"
      className="auth-corner pt-icon-btn"
      onClick={() => setTheme(isDark ? "light" : "dark")}
      title={isDark ? "Mode terang" : "Mode gelap"}
      aria-label={isDark ? "Ganti ke mode terang" : "Ganti ke mode gelap"}
    >
      {isDark ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );
}

// ── Password input with show/hide ────────────────────────────────────────────

interface PasswordFieldProps {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoComplete?: string;
  autoFocus?: boolean;
}

function PasswordField({
  id,
  label,
  value,
  onChange,
  placeholder = "••••••••",
  autoComplete = "current-password",
  autoFocus = false,
}: PasswordFieldProps) {
  const [show, setShow] = useState(false);
  return (
    <div className="auth-field">
      <label className="auth-label" htmlFor={id}>
        {label}
      </label>
      <div className="input-affix">
        <input
          id={id}
          type={show ? "text" : "password"}
          className="input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          autoComplete={autoComplete}
          autoFocus={autoFocus}
          aria-label={label}
        />
        <button
          type="button"
          className="input-affix-btn"
          onClick={() => setShow((s) => !s)}
          tabIndex={-1}
          aria-label={show ? "Sembunyikan sandi" : "Tampilkan sandi"}
        >
          {show ? <EyeOff size={15} /> : <Eye size={15} />}
        </button>
      </div>
    </div>
  );
}

// ── Setup form (first-run) ───────────────────────────────────────────────────

function SetupForm() {
  const { setup, loginDemo } = useAuth();
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const formRef = useRef<HTMLDivElement>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setPending(true);
    const result = await setup(pw, confirm);
    setPending(false);
    if (!result.ok) {
      setError(result.error ?? "Gagal membuat sandi.");
      // Shake animation
      if (formRef.current) {
        formRef.current.classList.remove("auth-shake");
        void formRef.current.offsetWidth; // reflow
        formRef.current.classList.add("auth-shake");
      }
    }
    // If ok, parent re-renders to AppShell (gate flips isUnlocked)
  }

  return (
    <div className="auth-card" ref={formRef}>
      <div className="auth-card-header">
        <h1 className="auth-card-title">Buat sandi master</h1>
        <p className="auth-card-sub">
          Buat sandi untuk melindungi data portofolio Anda.
          Sandi ini disimpan lokal di perangkat ini.
        </p>
      </div>
      {/* Using native form for semantics; onSubmit handles validation */}
      <form className="auth-form" onSubmit={handleSubmit} noValidate>
        <PasswordField
          id="setup-pw"
          label="Sandi baru"
          value={pw}
          onChange={setPw}
          placeholder="min. 6 karakter"
          autoComplete="new-password"
          autoFocus
        />
        <PasswordField
          id="setup-confirm"
          label="Konfirmasi sandi"
          value={confirm}
          onChange={setConfirm}
          placeholder="ulangi sandi"
          autoComplete="new-password"
        />
        <div className="auth-error" role="alert" aria-live="polite">
          {error && (
            <>
              <AlertCircle size={13} />
              {error}
            </>
          )}
        </div>
        <div className="auth-actions">
          <button
            type="submit"
            className="btn btn-primary w-full"
            disabled={pending}
          >
            {pending ? "Menyimpan…" : "Buat sandi & masuk"}
          </button>
          <div className="auth-divider">atau</div>
          <button
            type="button"
            className="btn btn-outline w-full"
            onClick={loginDemo}
          >
            Masuk dengan data demo
          </button>
        </div>
      </form>
    </div>
  );
}

// ── Login form (password exists) ─────────────────────────────────────────────

function LoginForm() {
  const { unlock, resetPassword, loginDemo } = useAuth();
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const formRef = useRef<HTMLDivElement>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setPending(true);
    const result = await unlock(pw);
    setPending(false);
    if (!result.ok) {
      setError(result.error ?? "Sandi salah.");
      setPw("");
      // Shake animation on wrong password
      if (formRef.current) {
        formRef.current.classList.remove("auth-shake");
        void formRef.current.offsetWidth;
        formRef.current.classList.add("auth-shake");
      }
    }
    // If ok, gate flips isUnlocked → AppShell renders
  }

  function handleReset() {
    if (
      window.confirm(
        "Reset sandi akan menghapus sandi tersimpan dan Anda perlu membuat sandi baru. Lanjutkan?",
      )
    ) {
      resetPassword();
    }
  }

  return (
    <div className="auth-card" ref={formRef}>
      <div className="auth-card-header">
        <h1 className="auth-card-title">Selamat datang kembali</h1>
        <p className="auth-card-sub">Masukkan sandi master untuk membuka portofolio Anda.</p>
      </div>
      <form className="auth-form" onSubmit={handleSubmit} noValidate>
        <PasswordField
          id="login-pw"
          label="Sandi master"
          value={pw}
          onChange={setPw}
          autoComplete="current-password"
          autoFocus
        />
        <div className="auth-error" role="alert" aria-live="polite">
          {error && (
            <>
              <AlertCircle size={13} />
              {error}
            </>
          )}
        </div>
        <div className="auth-actions">
          <button
            type="submit"
            className="btn btn-primary w-full"
            disabled={pending}
          >
            {pending ? "Memeriksa…" : "Masuk"}
          </button>
          <button
            type="button"
            className="auth-link"
            onClick={handleReset}
          >
            Lupa sandi?
          </button>
          <div className="auth-divider">atau</div>
          <button
            type="button"
            className="btn btn-outline w-full"
            onClick={loginDemo}
          >
            Masuk dengan data demo
          </button>
        </div>
      </form>
    </div>
  );
}

// ── LoginPage ────────────────────────────────────────────────────────────────

export default function LoginPage() {
  const { hasPassword } = useAuth();

  return (
    <div className="auth-shell">
      <BrandAside />
      <main className="auth-main">
        <ThemeCornerBtn />
        {hasPassword ? <LoginForm /> : <SetupForm />}
      </main>
    </div>
  );
}
