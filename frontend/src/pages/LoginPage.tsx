/**
 * LoginPage — fidelity-pass to match claude-design-source/app/page_auth.jsx
 *
 * Layout: auth-shell (CSS grid 1.05fr / 1fr) — aside left, form right.
 *
 * Aside:
 *   - Brand mark (pie icon gradient box) + "Portfolio"
 *   - Tagline: "Portofolio kamu, satu tampilan yang tenang."
 *   - Features: shield (Privat & self-hosted), scale (Dual currency), chat (WhatsApp)
 *   - Radial gradient + grid overlay via CSS (auth-aside::after)
 *   - © catalystlabs.id footer
 *
 * Form states:
 *   - First-run setup  (hasPassword === false) — "Buat sandi master" + confirm
 *   - Login            (hasPassword === true)  — unlock field + "Lupa sandi?"
 *
 * ⚠️ FRONTEND MOCK — NOT REAL SECURITY ⚠️
 */

import { useState, useRef, type FormEvent } from "react";
import {
  Eye,
  EyeOff,
  PieChart as PieIcon,
  Shield,
  Scale,
  MessageCircle,
  Sun,
  Moon,
  AlertCircle,
  Lock,
  ArrowRight,
} from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { useAuth } from "@/auth/AuthContext";

// ── Brand aside features (matches source FEATURES array) ────────────────────

const FEATURES = [
  {
    icon: Shield,
    title: "Privat & self-hosted",
    desc: "Data tetap di server kamu sendiri.",
  },
  {
    icon: Scale,
    title: "Dual currency IDR + USD",
    desc: "Nilai pasar otomatis di dua mata uang.",
  },
  {
    icon: MessageCircle,
    title: "Sinkron lewat WhatsApp",
    desc: "Tanya portofolio dari mana saja.",
  },
];

// ── Brand Aside ──────────────────────────────────────────────────────────────

function BrandAside() {
  return (
    <aside className="auth-aside">
      {/* Brand row */}
      <div className="flex items-center gap-3">
        <div className="auth-aside-mark">
          <PieIcon size={22} strokeWidth={2} />
        </div>
        <span className="auth-aside-name">Portfolio</span>
      </div>

      {/* Middle: tagline + features */}
      <div className="flex col gap-8">
        <h1 className="auth-tag">Portofolio kamu, satu tampilan yang tenang.</h1>
        <div className="auth-feat">
          {FEATURES.map((f) => (
            <div className="auth-feat-row" key={f.title}>
              <span className="auth-feat-ic">
                <f.icon size={19} strokeWidth={2} />
              </span>
              <div>
                <div className="t-sm" style={{ fontWeight: 600 }}>
                  {f.title}
                </div>
                <div className="t-xs t-muted">{f.desc}</div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Footer */}
      <div className="t-xs t-muted num">
        © {new Date().getFullYear()} catalystlabs.id · self-hosted
      </div>
    </aside>
  );
}

// ── Theme corner button ──────────────────────────────────────────────────────

function ThemeCornerBtn() {
  const { theme, setTheme } = useTheme();
  const isDark = theme === "dark";
  return (
    <div className="auth-corner">
      <button
        type="button"
        className="pt-icon-btn"
        onClick={() => setTheme(isDark ? "light" : "dark")}
        title={isDark ? "Mode terang" : "Mode gelap"}
        aria-label={isDark ? "Ganti ke mode terang" : "Ganti ke mode gelap"}
      >
        {isDark ? <Sun size={18} /> : <Moon size={18} />}
      </button>
    </div>
  );
}

// ── Password field with show/hide affix ─────────────────────────────────────

interface PasswordFieldProps {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  onClearError?: () => void;
  placeholder?: string;
  autoComplete?: string;
  autoFocus?: boolean;
}

function PasswordField({
  id,
  label,
  value,
  onChange,
  onClearError,
  placeholder = "••••••••",
  autoComplete = "current-password",
  autoFocus = false,
}: PasswordFieldProps) {
  const [show, setShow] = useState(false);
  return (
    <label className="field">
      <span className="field-label" id={`${id}-label`}>
        {label}
      </span>
      <div className="input-affix">
        <input
          id={id}
          type={show ? "text" : "password"}
          className="input"
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            onClearError?.();
          }}
          placeholder={placeholder}
          autoComplete={autoComplete}
          autoFocus={autoFocus}
          aria-label={label}
        />
        <button
          type="button"
          className="affix-btn"
          onClick={() => setShow((s) => !s)}
          tabIndex={-1}
          aria-label={show ? "Sembunyikan sandi" : "Tampilkan sandi"}
        >
          {show ? <EyeOff size={17} /> : <Eye size={17} />}
        </button>
      </div>
    </label>
  );
}

// ── Setup form (first-run) ───────────────────────────────────────────────────

function SetupForm() {
  const { setup, loginDemo } = useAuth();
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    const result = await setup(pw, confirm);
    setBusy(false);
    if (!result.ok) {
      setError(result.error ?? "Gagal membuat sandi.");
      if (formRef.current) {
        formRef.current.classList.remove("auth-shake");
        void formRef.current.offsetWidth;
        formRef.current.classList.add("auth-shake");
      }
    }
  }

  return (
    <form
      className="auth-card"
      onSubmit={handleSubmit}
      noValidate
      ref={formRef}
    >
      {/* Mobile brand — hidden on desktop (CSS .auth-mobile-brand) */}
      <div className="auth-mobile-brand">
        <div className="auth-aside-mark" style={{ width: 34, height: 34, borderRadius: 9 }}>
          <PieIcon size={19} strokeWidth={2} />
        </div>
        <span className="auth-aside-name" style={{ fontSize: 17 }}>
          Portfolio
        </span>
      </div>

      <div>
        <h2 className="t-h1">Buat sandi master</h2>
        <p className="t-sm t-muted" style={{ margin: "6px 0 0" }}>
          Sandi ini membuka akses ke portofolio kamu di perangkat ini.
        </p>
      </div>

      <div className="flex col gap-3">
        <PasswordField
          id="setup-pw"
          label="Sandi baru"
          value={pw}
          onChange={setPw}
          onClearError={() => setError("")}
          placeholder="Minimal 6 karakter"
          autoComplete="new-password"
          autoFocus
        />
        <PasswordField
          id="setup-confirm"
          label="Konfirmasi sandi"
          value={confirm}
          onChange={setConfirm}
          onClearError={() => setError("")}
          placeholder="Ulangi sandi"
          autoComplete="new-password"
        />
        <div
          className="auth-error"
          role="alert"
          aria-live="polite"
        >
          {error && (
            <>
              <AlertCircle size={14} />
              {error}
            </>
          )}
        </div>
      </div>

      <button
        type="submit"
        className="btn btn-primary"
        disabled={busy}
        style={{ width: "100%", height: 42 }}
      >
        {busy ? (
          "Memverifikasi…"
        ) : (
          <>
            <Lock size={16} />
            Buat &amp; masuk
            <ArrowRight size={16} />
          </>
        )}
      </button>

      <div className="flex items-center gap-3">
        <hr className="divider flex-1" />
        <span className="t-xs t-muted">atau</span>
        <hr className="divider flex-1" />
      </div>

      <button
        type="button"
        className="btn btn-outline"
        style={{ width: "100%" }}
        onClick={loginDemo}
      >
        Masuk dengan data demo
      </button>

      <div className="auth-foot-note">
        <Shield size={13} />
        Terenkripsi di perangkat · tidak ada server pihak ketiga
      </div>
    </form>
  );
}

// ── Login form (password exists) ─────────────────────────────────────────────

function LoginForm() {
  const { unlock, resetPassword, loginDemo } = useAuth();
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    const result = await unlock(pw);
    setBusy(false);
    if (!result.ok) {
      setError(result.error ?? "Sandi salah.");
      setPw("");
      if (formRef.current) {
        formRef.current.classList.remove("auth-shake");
        void formRef.current.offsetWidth;
        formRef.current.classList.add("auth-shake");
      }
    }
  }

  function handleForgot() {
    if (
      window.confirm(
        "Reset sandi akan menghapus sandi tersimpan dan Anda perlu membuat sandi baru. Lanjutkan?",
      )
    ) {
      resetPassword();
    }
  }

  return (
    <form
      className="auth-card"
      onSubmit={handleSubmit}
      noValidate
      ref={formRef}
    >
      {/* Mobile brand */}
      <div className="auth-mobile-brand">
        <div className="auth-aside-mark" style={{ width: 34, height: 34, borderRadius: 9 }}>
          <PieIcon size={19} strokeWidth={2} />
        </div>
        <span className="auth-aside-name" style={{ fontSize: 17 }}>
          Portfolio
        </span>
      </div>

      <div>
        <h2 className="t-h1">Selamat datang kembali</h2>
        <p className="t-sm t-muted" style={{ margin: "6px 0 0" }}>
          Masukkan sandi master untuk membuka portofolio.
        </p>
      </div>

      <div className="flex col gap-3">
        <PasswordField
          id="login-pw"
          label="Sandi master"
          value={pw}
          onChange={setPw}
          onClearError={() => setError("")}
          autoComplete="current-password"
          autoFocus
        />
        <div
          className="auth-error"
          role="alert"
          aria-live="polite"
        >
          {error && (
            <>
              <AlertCircle size={14} />
              {error}
            </>
          )}
        </div>
        <button
          type="button"
          className="auth-link"
          onClick={handleForgot}
        >
          Lupa sandi?
        </button>
      </div>

      <button
        type="submit"
        className="btn btn-primary"
        disabled={busy}
        style={{ width: "100%", height: 42 }}
      >
        {busy ? (
          "Memverifikasi…"
        ) : (
          <>
            <Lock size={16} />
            Buka portofolio
            <ArrowRight size={16} />
          </>
        )}
      </button>

      <div className="flex items-center gap-3">
        <hr className="divider flex-1" />
        <span className="t-xs t-muted">atau</span>
        <hr className="divider flex-1" />
      </div>

      <button
        type="button"
        className="btn btn-outline"
        style={{ width: "100%" }}
        onClick={loginDemo}
      >
        Masuk dengan data demo
      </button>

      <div className="auth-foot-note">
        <Shield size={13} />
        Terenkripsi di perangkat · tidak ada server pihak ketiga
      </div>
    </form>
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
