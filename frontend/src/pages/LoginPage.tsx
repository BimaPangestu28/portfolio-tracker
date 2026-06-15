/**
 * LoginPage — fidelity-pass to match claude-design-source/app/page_auth.jsx
 *
 * Layout: auth-shell (CSS grid 1.05fr / 1fr) — aside left, form right.
 *
 * Aside:
 *   - Brand mark (sparkles gradient box) + "Noah"
 *   - Tagline: "Asisten pribadi kamu, dalam satu tempat yang tenang."
 *   - Features: shield (Privat & self-hosted), scale (Dual currency), chat (WhatsApp)
 *   - Radial gradient + grid overlay via CSS (auth-aside::after)
 *   - © catalystlabs.id footer
 *
 * Form: a single master-password field. The password is exchanged at
 * POST /auth/login for a JWT (see AuthContext.unlock); a 401 surfaces the
 * server's error message inline.
 */

import { useState, useRef, type FormEvent } from "react";
import {
  Eye,
  EyeOff,
  Sparkles as PieIcon,
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
    desc: "Tanya Noah dari mana saja.",
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
        <span className="auth-aside-name">Noah</span>
      </div>

      {/* Middle: tagline + features */}
      <div className="flex col gap-8">
        <h1 className="auth-tag">Asisten pribadi kamu, dalam satu tempat yang tenang.</h1>
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

// ── Login form (single master-password field) ────────────────────────────────

function LoginForm() {
  const { unlock } = useAuth();
  const [pw, setPw] = useState("");
  const [show, setShow] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const result = await unlock(pw);
    setBusy(false);
    if (!result.ok) {
      setError(result.error ?? "Gagal masuk.");
      setPw("");
      if (formRef.current) {
        formRef.current.classList.remove("auth-shake");
        void formRef.current.offsetWidth;
        formRef.current.classList.add("auth-shake");
      }
    }
  }

  return (
    <form className="auth-card" onSubmit={handleSubmit} noValidate ref={formRef}>
      {/* Mobile brand — hidden on desktop (CSS .auth-mobile-brand) */}
      <div className="auth-mobile-brand">
        <div className="auth-aside-mark" style={{ width: 34, height: 34, borderRadius: 9 }}>
          <PieIcon size={19} strokeWidth={2} />
        </div>
        <span className="auth-aside-name" style={{ fontSize: 17 }}>
          Noah
        </span>
      </div>

      <div>
        <h2 className="t-h1">Selamat datang kembali</h2>
        <p className="t-sm t-muted" style={{ margin: "6px 0 0" }}>
          Masukkan sandi master untuk membuka portofolio.
        </p>
      </div>

      <div className="flex col gap-3">
        <div className="field">
          <label className="field-label" htmlFor="master-pw">
            Sandi master
          </label>
          <div className="input-affix">
            <input
              id="master-pw"
              type={show ? "text" : "password"}
              className="input"
              value={pw}
              onChange={(e) => {
                setPw(e.target.value);
                setError(null);
              }}
              placeholder="••••••••"
              autoComplete="current-password"
              autoFocus
            />
            <button
              type="button"
              className="affix-btn"
              onClick={() => setShow((s) => !s)}
              tabIndex={-1}
              aria-label={show ? "Sembunyikan" : "Tampilkan"}
            >
              {show ? <EyeOff size={17} /> : <Eye size={17} />}
            </button>
          </div>
        </div>
        <div className="auth-error" role="alert" aria-live="polite">
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
        disabled={busy || !pw}
        style={{ width: "100%", height: 42 }}
        aria-label="Masuk"
      >
        {busy ? (
          "Membuka…"
        ) : (
          <>
            <Lock size={16} />
            Masuk
            <ArrowRight size={16} />
          </>
        )}
      </button>

      <div className="auth-foot-note">
        <Shield size={13} />
        Token JWT · diverifikasi oleh server kamu
      </div>
    </form>
  );
}

// ── LoginPage ────────────────────────────────────────────────────────────────

export default function LoginPage() {
  return (
    <div className="auth-shell">
      <BrandAside />
      <main className="auth-main">
        <ThemeCornerBtn />
        <LoginForm />
      </main>
    </div>
  );
}
