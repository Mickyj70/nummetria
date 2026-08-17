const principles = [
  {
    number: "01",
    title: "Local by default",
    copy: "Usage stays on your machine in SQLite. Prompts and responses are never collected.",
  },
  {
    number: "02",
    title: "Evidence, not guesses",
    copy: "Every cost is labeled as reported, calculated, estimated, or unknown.",
  },
  {
    number: "03",
    title: "Built in the open",
    copy: "A documented Rust core, stable JSON output, and provider adapters designed for contributors.",
  },
];

const roadmap = [
  ["Now", "Cross-platform CLI foundation", "Windows + macOS"],
  ["Next", "OpenAI and Anthropic adapters", "Trusted usage data"],
  ["Then", "Budgets, exports, and plugins", "Community extensibility"],
  ["Later", "Menu bar and notch companion", "Glanceable monitoring"],
];

export default function Home() {
  return (
    <main>
      <nav className="nav shell" aria-label="Primary navigation">
        <a className="wordmark" href="#top" aria-label="Nummetria home">
          <span className="wordmark-mark" aria-hidden="true">N</span>
          <span>Nummetria</span>
        </a>
        <div className="nav-links">
          <a href="#principles">Principles</a>
          <a href="#roadmap">Roadmap</a>
          <a className="nav-status" href="#open-source">
            <span aria-hidden="true" /> Open source
          </a>
        </div>
      </nav>

      <section className="hero shell" id="top">
        <div className="hero-copy">
          <p className="eyebrow">AI usage intelligence · local first</p>
          <h1>
            Every model.<br />
            Every token.<br />
            <span>One honest ledger.</span>
          </h1>
          <p className="hero-lede">
            Nummetria is the open-source command center for understanding what
            your AI tools consume—across providers, projects, and models.
          </p>
          <div className="hero-actions">
            <a className="button button-primary" href="#roadmap">Follow the build</a>
            <a className="button button-secondary" href="#scope">Explore v0.1</a>
          </div>
          <div className="hero-note">
            <span>Rust core</span>
            <span>Windows + macOS</span>
            <span>No telemetry</span>
          </div>
        </div>

        <div className="terminal-wrap" aria-label="Nummetria command line preview">
          <div className="terminal">
            <div className="terminal-bar">
              <span className="terminal-title">nummetria / today</span>
              <span className="terminal-live"><i /> live</span>
            </div>
            <div className="terminal-command"><b>$</b> nummetria status</div>
            <div className="terminal-heading">
              <span>Usage · today</span>
              <span>Updated 14s ago</span>
            </div>
            <div className="metric-row metric-main">
              <div><small>SPEND</small><strong>$3.42</strong></div>
              <div><small>TOKENS</small><strong>2.03M</strong></div>
              <div><small>REQUESTS</small><strong>184</strong></div>
            </div>
            <div className="provider-table">
              <div className="table-head"><span>PROVIDER</span><span>USAGE</span><span>COST</span></div>
              <div><span><i className="provider-dot dot-green" />OpenAI</span><span>1.20M</span><span>$1.84</span></div>
              <div><span><i className="provider-dot dot-orange" />Anthropic</span><span>640K</span><span>$1.21</span></div>
              <div><span><i className="provider-dot dot-blue" />Other</span><span>190K</span><span>$0.37</span></div>
            </div>
            <div className="budget-label"><span>Daily budget</span><span>$3.42 / $5.00</span></div>
            <div className="budget-track"><span /></div>
            <div className="terminal-foot"><span>Reported costs</span><span>68% used</span></div>
          </div>
          <p className="terminal-caption"><span>01</span> One normalized view across every connected provider.</p>
        </div>
      </section>

      <section className="ledger-strip" aria-label="Nummetria capabilities">
        <div className="shell ledger-grid">
          <div><small>TRACK</small><strong>Tokens · cost · requests</strong></div>
          <div><small>COMPARE</small><strong>Providers · models · projects</strong></div>
          <div><small>CONTROL</small><strong>Budgets · alerts · exports</strong></div>
        </div>
      </section>

      <section className="principles shell" id="principles">
        <div className="section-intro">
          <p className="eyebrow">Designed for trust</p>
          <h2>Usage data should be useful<br />without becoming surveillance.</h2>
          <p>Nummetria records operational metadata—not the work itself.</p>
        </div>
        <div className="principle-grid">
          {principles.map((principle) => (
            <article key={principle.number}>
              <span>{principle.number}</span>
              <h3>{principle.title}</h3>
              <p>{principle.copy}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="scope" id="scope">
        <div className="shell scope-grid">
          <div>
            <p className="eyebrow">Version 0.1</p>
            <h2>A small first release.<br />A serious foundation.</h2>
          </div>
          <div className="scope-list">
            <div><span>01</span><p><strong>Collect</strong> official usage from OpenAI and Anthropic.</p></div>
            <div><span>02</span><p><strong>Understand</strong> daily, weekly, and monthly consumption.</p></div>
            <div><span>03</span><p><strong>Protect</strong> credentials with native secure storage.</p></div>
            <div><span>04</span><p><strong>Automate</strong> with stable JSON and CSV exports.</p></div>
          </div>
        </div>
      </section>

      <section className="roadmap shell" id="roadmap">
        <div className="section-intro roadmap-intro">
          <p className="eyebrow">Build sequence</p>
          <h2>CLI first. Native surfaces next.</h2>
          <p>We are proving the data model and cross-platform experience before adding desktop chrome.</p>
        </div>
        <div className="roadmap-list">
          {roadmap.map(([phase, title, detail], index) => (
            <div className="roadmap-row" key={phase}>
              <span className="roadmap-index">{String(index + 1).padStart(2, "0")}</span>
              <span className={`phase phase-${index}`}>{phase}</span>
              <strong>{title}</strong>
              <span className="roadmap-detail">{detail}</span>
            </div>
          ))}
        </div>
      </section>

      <section className="open-source" id="open-source">
        <div className="shell open-source-inner">
          <div>
            <p className="eyebrow">Built to be extended</p>
            <h2>Your tools will change.<br />Your ledger should adapt.</h2>
          </div>
          <div>
            <p>
              Nummetria will ship with documented provider contracts, test
              fixtures, and contributor guides so the community can add the
              next integration without rebuilding the core.
            </p>
            <a className="text-link" href="#roadmap">See the open-source roadmap <span>↗</span></a>
          </div>
        </div>
      </section>

      <footer className="footer shell">
        <a className="wordmark" href="#top"><span className="wordmark-mark">N</span><span>Nummetria</span></a>
        <p>Know what your AI tools consume.</p>
        <span>Open-source · local-first · 2026</span>
      </footer>
    </main>
  );
}
