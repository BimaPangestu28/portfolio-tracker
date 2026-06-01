/* ============================================================
   Auth — Login + first-run setup + lock screen
   (single-user, self-hosted: unlock with master password)
   ============================================================ */
const { useState: auState } = React;

function Login({ onAuth, theme, setTheme }) {
  const hasPass = !!localStorage.getItem('pt-pass');
  const [mode, setMode] = auState(hasPass ? 'login' : 'setup');
  const [pw, setPw] = auState('');
  const [pw2, setPw2] = auState('');
  const [show, setShow] = auState(false);
  const [err, setErr] = auState('');
  const [busy, setBusy] = auState(false);

  const fail = (m) => { setErr(m); setBusy(false); };
  const submit = (e) => {
    if (e) e.preventDefault();
    setErr('');
    if (mode === 'setup') {
      if (pw.length < 6) return fail('Sandi minimal 6 karakter.');
      if (pw !== pw2) return fail('Konfirmasi sandi tidak cocok.');
      setBusy(true);
      setTimeout(() => { localStorage.setItem('pt-pass', pw); onAuth(); }, 650);
    } else {
      setBusy(true);
      setTimeout(() => {
        if (pw === localStorage.getItem('pt-pass')) onAuth();
        else fail('Sandi salah. Coba lagi.');
      }, 650);
    }
  };
  const demo = () => { if (!localStorage.getItem('pt-pass')) localStorage.setItem('pt-pass', 'demo123'); onAuth(); };
  const forgot = () => window.toast('Atur ulang lewat berkas .env pada instance self-hosted kamu.', 'info');

  const FEATURES = [
    { icon: 'shield', t: 'Privat & self-hosted', s: 'Data tetap di server kamu sendiri.' },
    { icon: 'scale', t: 'Dual currency IDR + USD', s: 'Nilai pasar otomatis di dua mata uang.' },
    { icon: 'chat', t: 'Sinkron lewat WhatsApp', s: 'Tanya portofolio dari mana saja.' },
  ];

  return (
    <div className="auth-shell">
      <aside className="auth-aside">
        <div className="flex items-center gap-3">
          <div className="brand-mark" style={{ width: 40, height: 40 }}><Icon name="pie" size={22} /></div>
          <span className="brand-name" style={{ fontSize: 18 }}>Portfolio</span>
        </div>
        <div className="flex col gap-8">
          <h1 className="auth-tag">Portofolio kamu, satu tampilan yang tenang.</h1>
          <div className="auth-feat">
            {FEATURES.map(f => (
              <div className="auth-feat-row" key={f.t}>
                <span className="auth-feat-ic"><Icon name={f.icon} size={19} /></span>
                <div>
                  <div className="t-sm" style={{ fontWeight: 600 }}>{f.t}</div>
                  <div className="t-xs t-muted">{f.s}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="t-xs t-muted num">© {new Date().getFullYear()} catalystlabs.id · self-hosted</div>
      </aside>

      <main className="auth-main">
        <div className="auth-corner">
          <button className="icon-btn" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')} title="Tema">
            <Icon name={theme === 'dark' ? 'sun' : 'moon'} size={18} />
          </button>
        </div>

        <form className={'auth-card' + (err ? ' shake' : '')} onSubmit={submit} key={err + mode}>
          <div className="auth-mobile-brand">
            <div className="brand-mark"><Icon name="pie" size={19} /></div>
            <span className="brand-name" style={{ fontSize: 17 }}>Portfolio</span>
          </div>

          <div>
            <h2 className="t-h1">{mode === 'setup' ? 'Buat sandi master' : 'Selamat datang kembali'}</h2>
            <p className="t-sm t-muted" style={{ margin: '6px 0 0' }}>
              {mode === 'setup' ? 'Sandi ini membuka akses ke portofolio kamu di perangkat ini.' : 'Masukkan sandi master untuk membuka portofolio.'}
            </p>
          </div>

          <div className="flex col gap-3">
            <Field label="Sandi master">
              <div className="input-affix">
                <input className="input" type={show ? 'text' : 'password'} value={pw} autoFocus
                  placeholder={mode === 'setup' ? 'Minimal 6 karakter' : '••••••••'}
                  onChange={e => { setPw(e.target.value); setErr(''); }} />
                <button type="button" className="affix-btn" onClick={() => setShow(s => !s)} tabIndex={-1} aria-label="Tampilkan sandi">
                  <Icon name={show ? 'eyeOff' : 'eye'} size={17} />
                </button>
              </div>
            </Field>
            {mode === 'setup' && (
              <Field label="Konfirmasi sandi">
                <input className="input" type={show ? 'text' : 'password'} value={pw2} placeholder="Ulangi sandi"
                  onChange={e => { setPw2(e.target.value); setErr(''); }} />
              </Field>
            )}
            {err && <div className="t-xs loss flex items-center gap-2" style={{ fontWeight: 540 }}><Icon name="alertCircle" size={14} />{err}</div>}
            {mode === 'login' && (
              <button type="button" className="t-xs" style={{ alignSelf: 'flex-end', color: 'hsl(var(--primary))', background: 'none', border: 'none', cursor: 'pointer', padding: 0, fontWeight: 540 }} onClick={forgot}>Lupa sandi?</button>
            )}
          </div>

          <Button type="submit" variant="primary" disabled={busy} icon={busy ? 'refresh' : 'lock'} iconRight={busy ? null : 'arrowRight'} style={{ width: '100%', height: 42 }}>
            {busy ? 'Memverifikasi…' : (mode === 'setup' ? 'Buat & masuk' : 'Buka portofolio')}
          </Button>

          <div className="flex items-center gap-3">
            <hr className="divider flex-1" />
            <span className="t-xs t-muted">atau</span>
            <hr className="divider flex-1" />
          </div>
          <button type="button" className="btn btn-outline" style={{ width: '100%' }} onClick={demo}>
            <Icon name="sparkles" size={15} />Masuk dengan data demo
          </button>

          <div className="auth-foot-note"><Icon name="shield" size={13} />Terenkripsi di perangkat · tidak ada server pihak ketiga</div>
        </form>
      </main>
    </div>
  );
}
window.Login = Login;
