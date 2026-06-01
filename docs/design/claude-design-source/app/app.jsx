/* ============================================================
   App shell — sidebar, topbar, mobile nav, router, theme
   ============================================================ */
const { useState: aState, useEffect: aEffect } = React;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "accent": "#3b82f6",
  "density": "comfortable",
  "radius": "soft"
}/*EDITMODE-END*/;
const ACCENT_HUE = { '#3b82f6': '217', '#10b981': '160', '#8b5cf6': '262', '#06b6d4': '190' };

const NAV = [
  { key: 'dashboard', label: 'Dashboard', icon: 'dashboard' },
  { key: 'portfolio', label: 'Portofolio', icon: 'wallet' },
  { key: 'planner', label: 'Rencana', icon: 'target' },
  { key: 'budget', label: 'Budget', icon: 'banknote' },
  { key: 'data', label: 'Data', icon: 'inbox' },
  { key: 'chat', label: 'Chat', icon: 'chat' },
];
const ROUTE_ALIAS = { holdings: 'portfolio', transactions: 'portfolio', connectors: 'data', import: 'data' };
const TITLES = Object.fromEntries(NAV.map(n => [n.key, n.label]));
const BOTTOM = ['dashboard', 'portfolio', 'budget', 'chat'];

function NavList({ route, go, collapsed }) {
  const needs = window.PT.IMPORT_QUEUE.filter(q => q.needs).length;
  return (
    <nav className="nav">
      {NAV.map((n) => (
        <button key={n.key} className={route === n.key ? 'nav-item active' : 'nav-item'} onClick={() => go(n.key)} title={collapsed ? n.label : ''}>
          <Icon name={n.icon} size={18} />
          <span className="nav-label">{n.label}</span>
          {n.key === 'data' && needs > 0 && <span className="badge-dot nav-label" style={{ background: 'hsl(var(--warn))', marginLeft: 'auto' }}></span>}
        </button>
      ))}
    </nav>
  );
}

function App() {
  const getRoute = () => { const r = location.hash.replace('#/', '') || 'dashboard'; return ROUTE_ALIAS[r] || r; };
  const [route, setRoute] = aState(getRoute());
  const [theme, setTheme] = aState(localStorage.getItem('pt-theme') || 'dark');
  const [base, setBase] = aState(localStorage.getItem('pt-base') || 'IDR');
  const [collapsed, setCollapsed] = aState(false);
  const [sheet, setSheet] = aState(false);
  const [loading, setLoading] = aState(true);
  const [authed, setAuthed] = aState(localStorage.getItem('pt-authed') === '1');
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);

  aEffect(() => {
    const root = document.documentElement;
    root.style.setProperty('--primary-h', ACCENT_HUE[t.accent] || '217');
    root.classList.toggle('dense', t.density === 'compact');
    root.classList.toggle('sharp', t.radius === 'sharp');
  }, [t.accent, t.density, t.radius]);

  aEffect(() => {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    localStorage.setItem('pt-theme', theme);
  }, [theme]);
  aEffect(() => { localStorage.setItem('pt-base', base); }, [base]);
  aEffect(() => {
    const h = () => setRoute(getRoute());
    window.addEventListener('hashchange', h);
    const t = setTimeout(() => setLoading(false), 700);
    return () => { window.removeEventListener('hashchange', h); clearTimeout(t); };
  }, []);

  const go = (k) => { location.hash = '/' + k; setRoute(k); setSheet(false); };
  const refresh = () => { setLoading(true); window.toast('Memperbarui harga…', 'info'); setTimeout(() => { setLoading(false); window.toast('Harga diperbarui', 'success'); }, 850); };
  const authIn = () => { localStorage.setItem('pt-authed', '1'); setAuthed(true); };
  const lock = () => { localStorage.removeItem('pt-authed'); setSheet(false); setAuthed(false); window.toast('Portofolio dikunci', 'info'); };

  if (!authed) {
    return (
      <React.Fragment>
        <Login onAuth={authIn} theme={theme} setTheme={setTheme} />
        <ToastHost />
      </React.Fragment>
    );
  }

  const PAGES = {
    dashboard: <Dashboard loading={loading} base={base} />,
    portfolio: <Portfolio loading={loading} />,
    planner: <Planner loading={loading} />,
    budget: <Budget loading={loading} />,
    data: <DataHub />,
    chat: <Chat />,
  };

  return (
    <div className="app-shell">
      {/* desktop sidebar */}
      <aside className={'sidebar ' + (collapsed ? 'collapsed' : '')}>
        <div className="brand">
          <div className="brand-mark"><Icon name="pie" size={19} /></div>
          <span className="brand-name nav-label">Portfolio</span>
        </div>
        <NavList route={route} go={go} collapsed={collapsed} />
        <div className="sidebar-foot">
          <button className="nav-item" onClick={lock} title="Kunci">
            <Icon name="lock" size={18} /><span className="nav-label">Kunci portofolio</span>
          </button>
          <button className="nav-item" onClick={() => setCollapsed(c => !c)} title="Ciutkan">
            <Icon name="panelLeft" size={18} /><span className="nav-label">Ciutkan</span>
          </button>
          <div className="nav-label t-xs t-muted" style={{ padding: '8px 12px 2px' }}>© 2026 catalystlabs.id</div>
        </div>
      </aside>

      {/* mobile sheet */}
      {sheet && (
        <React.Fragment>
          <div className="scrim" onClick={() => setSheet(false)}></div>
          <div className="sheet-left">
            <div className="brand"><div className="brand-mark"><Icon name="pie" size={19} /></div><span className="brand-name">Portfolio</span>
              <button className="icon-btn" style={{ marginLeft: 'auto' }} onClick={() => setSheet(false)}><Icon name="x" /></button></div>
            <NavList route={route} go={go} />
            <div className="sidebar-foot">
              <button className="nav-item" onClick={lock} title="Kunci"><Icon name="lock" size={18} /><span>Kunci portofolio</span></button>
            </div>
          </div>
        </React.Fragment>
      )}

      <div className="main">
        <header className="topbar">
          <button className="icon-btn hamburger" onClick={() => setSheet(true)}><Icon name="menu" /></button>
          <div className="t-h2 flex-1 truncate">{TITLES[route]}</div>
          <Seg value={base} onChange={setBase} options={[{ value: 'IDR', label: 'IDR' }, { value: 'USD', label: 'USD' }]} />
          <button className="icon-btn" onClick={refresh} title="Perbarui harga"><Icon name="refresh" size={18} /></button>
          <button className="icon-btn" onClick={() => setTheme(t => t === 'dark' ? 'light' : 'dark')} title="Tema"><Icon name={theme === 'dark' ? 'sun' : 'moon'} size={18} /></button>
          <button className="btn btn-outline btn-sm" onClick={() => go('chat')} style={{ marginLeft: 4 }}><Icon name="sparkles" size={15} />Tanya</button>
        </header>
        <div className="page-scroll">
          <div className="page">{PAGES[route] || PAGES.dashboard}</div>
        </div>
      </div>

      {/* mobile bottom nav */}
      <nav className="bottom-nav">
        {BOTTOM.map(k => { const n = NAV.find(x => x.key === k); return (
          <button key={k} className={'tab ' + (route === k ? 'active' : '')} onClick={() => go(k)}><Icon name={n.icon} size={20} /><span>{n.label}</span></button>
        ); })}
        <button className={'tab ' + (!BOTTOM.includes(route) ? 'active' : '')} onClick={() => setSheet(true)}><Icon name="menu" size={20} /><span>Lainnya</span></button>
      </nav>

      <ToastHost />
      <TweaksPanel>
        <TweakSection label="Tampilan" />
        <TweakColor label="Warna aksen" value={t.accent} options={['#3b82f6', '#10b981', '#8b5cf6', '#06b6d4']} onChange={(v) => setTweak('accent', v)} />
        <TweakRadio label="Kepadatan" value={t.density} options={['comfortable', 'compact']} onChange={(v) => setTweak('density', v)} />
        <TweakRadio label="Sudut" value={t.radius} options={['soft', 'sharp']} onChange={(v) => setTweak('radius', v)} />
        <TweakSection label="Tema" />
        <TweakToggle label="Mode gelap" value={theme === 'dark'} onChange={(v) => setTheme(v ? 'dark' : 'light')} />
        <TweakRadio label="Mata uang utama" value={base} options={['IDR', 'USD']} onChange={setBase} />
      </TweaksPanel>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
