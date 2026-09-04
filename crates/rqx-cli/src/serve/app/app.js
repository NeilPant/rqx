/* The dashboard. One file, no build step: the binary that serves it promises no
   runtime dependencies, and a page that needed a bundler to exist would be a second
   toolchain in a repository that has one. */

const { h, render } = preact;
const { useState, useEffect, useMemo, useRef, useCallback } = preactHooks;
const html = htm.bind(h);

const GRADES = ["pristine", "clean", "fair", "poor", "critical"];
const GRADE_INK = {
  pristine: "var(--accent)",
  clean: "var(--good)",
  fair: "var(--warn)",
  poor: "#e07a2b",
  critical: "var(--bad)",
};
const SEVERITY_INK = { error: "var(--bad)", warning: "var(--warn)", info: "var(--info)" };

const PAGES = [
  ["overview", "Overview"],
  ["map", "Map"],
  ["findings", "Findings"],
  ["files", "Files"],
  ["plan", "Plan"],
  ["rules", "Rules"],
  ["activity", "Activity"],
];

const json = (url) => fetch(url).then((answer) => answer.json());
const shortPath = (path) => path.split("/").slice(-2).join("/");

/* A number that moves rather than jumps, so a change registers as a change. The
   duration is short enough that nobody waits to read the real value. */
function useCounted(target) {
  const [shown, setShown] = useState(target);
  const from = useRef(target);
  useEffect(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setShown(target);
      return;
    }
    const start = performance.now();
    const origin = from.current;
    let frame;
    const step = (now) => {
      const progress = Math.min((now - start) / 420, 1);
      const eased = 1 - Math.pow(1 - progress, 3);
      setShown(Math.round(origin + (target - origin) * eased));
      if (progress < 1) frame = requestAnimationFrame(step);
      else from.current = target;
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [target]);
  return shown;
}

function scoreInk(score) {
  if (score >= 90) return "var(--good)";
  if (score >= 70) return "var(--warn)";
  return "var(--bad)";
}

/* ── overview ─────────────────────────────────────────────────────────── */

function Sparkline({ points }) {
  if (points.length < 2) {
    return html`<div class="sub">A trend appears once the project changes.</div>`;
  }
  const width = 480;
  const height = 68;
  const values = points.map((point) => point.health);
  const low = Math.min(...values, 0);
  const high = Math.max(...values, 100);
  const span = Math.max(high - low, 1);
  const path = points
    .map((point, index) => {
      const x = (index / (points.length - 1)) * width;
      const y = height - ((point.health - low) / span) * (height - 8) - 4;
      return `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const last = points[points.length - 1].health;
  return html`
    <svg class="spark" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none"
         role="img" aria-label="Health over this session, now ${last}">
      <path d=${path} fill="none" stroke=${scoreInk(last)} stroke-width="2"
            stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  `;
}

function Bars({ counts, total, ink, order }) {
  const largest = Math.max(...order.map((key) => counts[key] || 0), 1);
  return html`
    <div class="bars">
      ${order
        .filter((key) => counts[key])
        .map(
          (key) => html`
            <div class="bar-row" key=${key}>
              <div style="text-transform:capitalize">${key}</div>
              <div class="bar-track">
                <div class="bar-fill"
                     style="width:${((counts[key] || 0) / largest) * 100}%;background:${ink[key]}"></div>
              </div>
              <div class="bar-count">${counts[key] || 0}</div>
            </div>
          `,
        )}
      ${total === 0 && html`<div class="sub">Nothing to report.</div>`}
    </div>
  `;
}

function Overview({ report, trend, levels, go }) {
  const health = useCounted(report.health || 0);
  const grades = report.grades || {};
  const gradeCounts = {};
  for (const grade of Object.values(grades)) gradeCounts[grade] = (gradeCounts[grade] || 0) + 1;
  const severities = report.counts?.by_severity || {};
  const start = report.startHere || [];

  return html`
    <div class="page-head">
      <h1>Overview</h1>
      <div class="sub">${report.filesScanned} files · ${report.codeLines} lines · ${report.elapsedMs} ms</div>
    </div>

    <div class="hero">
      <div>
        <div class="score">
          <div class="score-value" style="color:${scoreInk(report.health || 0)}">${health}</div>
          <div class="score-of">/ 100</div>
        </div>
        <div class="score-label">
          ${report.counts?.total || 0} findings across ${Object.keys(grades).length} files
        </div>
      </div>
      <${Sparkline} points=${trend} />
    </div>

    <div class="cards">
      <div class="card">
        <div class="card-label">Errors</div>
        <div class="card-value" style="color:var(--bad)">${severities.error || 0}</div>
        <div class="card-note">never a false positive</div>
      </div>
      <div class="card">
        <div class="card-label">Warnings</div>
        <div class="card-value" style="color:var(--warn)">${severities.warning || 0}</div>
      </div>
      <div class="card">
        <div class="card-label">Auto-fixable</div>
        <div class="card-value" style="color:var(--good)">
          ${(report.findings || []).filter((finding) => finding.autofixable).length}
        </div>
        <div class="card-note">rqx fix --write</div>
      </div>
      <div class="card">
        <div class="card-label">Level</div>
        <div class="card-value" style="text-transform:capitalize;font-size:22px">${report.level}</div>
        <div class="card-note">rqx levels</div>
      </div>
    </div>

    <section>
      <h2>Grades</h2>
      <${Bars} counts=${gradeCounts} total=${Object.keys(grades).length}
               ink=${GRADE_INK} order=${GRADES} />
    </section>

    <section>
      <h2>What it would score at each level</h2>
      <div class="sub" style="margin:-6px 0 12px">
        Counted from the findings in hand: a level also moves thresholds, and a moved
        threshold decides whether a finding exists at all, so run
        <code>rqx scan --level strict</code> for the exact answer.
      </div>
      <div class="levels">
        ${levels.map(
          (level) => html`
            <div class="level" key=${level.level} data-current=${String(level.current)}>
              <div class="level-name">${level.level}</div>
              <div class="level-score" style="color:${scoreInk(level.health)}">${level.health}</div>
              <div class="level-note">${level.findings} findings · ${level.summary}</div>
            </div>
          `,
        )}
      </div>
    </section>

    <section>
      <h2>Start here</h2>
      <div class="rows">
        ${start.slice(0, 8).map(
          (file) => html`
            <button class="row row-files" key=${file.file} onClick=${() => go("files")}>
              <div class="path" title=${file.file}>${file.file}</div>
              <div style="color:${GRADE_INK[file.grade]}">${file.grade}</div>
              <div class="num">${file.findings}</div>
              <div class="num">${file.importers} in</div>
            </button>
          `,
        )}
        ${start.length === 0 && html`<div class="empty">Nothing needs opening first.</div>`}
      </div>
    </section>
  `;
}

/* ── treemap ──────────────────────────────────────────────────────────── */

/* Squarified enough to be readable: split the run whose aspect ratios stay closest
   to square, which is the whole reason a treemap beats a bar chart here. */
function squarify(items, x, y, width, height, out) {
  if (items.length === 0) return;
  if (items.length === 1) {
    out.push({ ...items[0], x, y, width, height });
    return;
  }
  const total = items.reduce((sum, item) => sum + item.value, 0);
  let taken = 0;
  let split = 0;
  const half = total / 2;
  while (split < items.length - 1 && taken + items[split].value <= half) {
    taken += items[split].value;
    split += 1;
  }
  const head = items.slice(0, split || 1);
  const tail = items.slice(split || 1);
  const share = taken / total || 0.5;
  if (width >= height) {
    const cut = width * share;
    squarify(head, x, y, cut, height, out);
    squarify(tail, x + cut, y, width - cut, height, out);
  } else {
    const cut = height * share;
    squarify(head, x, y, width, cut, out);
    squarify(tail, x, y + cut, width, height - cut, out);
  }
}

function Treemap({ report, openFile }) {
  const [box, setBox] = useState({ width: 900, height: 520 });
  const frame = useRef(null);

  useEffect(() => {
    const measure = () => {
      if (frame.current) {
        setBox({ width: frame.current.clientWidth, height: 520 });
      }
    };
    measure();
    addEventListener("resize", measure);
    return () => removeEventListener("resize", measure);
  }, []);

  const tiles = useMemo(() => {
    const grades = report.grades || {};
    const lines = report.lines || {};
    const items = Object.keys(grades)
      .map((file) => ({
        file,
        grade: grades[file],
        value: Math.max(lines[file] || 1, 1),
      }))
      .sort((left, right) => right.value - left.value);
    const out = [];
    squarify(items, 0, 0, box.width, box.height, out);
    return out;
  }, [report, box]);

  return html`
    <div class="page-head">
      <h1>Map</h1>
      <div class="sub">every file, sized by lines, coloured by grade</div>
    </div>
    <div class="map" ref=${frame} style="position:relative;height:${box.height}px">
      ${tiles.map(
        (tile) => html`
          <button class="tile" key=${tile.file}
                  onClick=${() => openFile(tile.file)}
                  title="${tile.file} · ${tile.grade} · ${tile.value} lines"
                  style="left:${tile.x}px;top:${tile.y}px;width:${tile.width}px;height:${tile.height}px;background:${GRADE_INK[tile.grade]}">
            ${tile.width > 56 && tile.height > 18 &&
            html`<div class="tile-label">${shortPath(tile.file)}</div>`}
          </button>
        `,
      )}
    </div>
    <section>
      <h2>Grade</h2>
      <div class="filters">
        ${GRADES.map(
          (grade) => html`
            <span class="chip" key=${grade}>
              <span class="pip" style="display:inline-block;background:${GRADE_INK[grade]};margin-right:6px"></span>
              ${grade}
            </span>
          `,
        )}
      </div>
    </section>
  `;
}

/* ── findings ─────────────────────────────────────────────────────────── */

function Findings({ report, openFinding }) {
  const [severity, setSeverity] = useState(null);
  const [family, setFamily] = useState(null);
  const [fixable, setFixable] = useState(false);
  const [term, setTerm] = useState("");

  const findings = report.findings || [];
  const families = useMemo(
    () => [...new Set(findings.map((finding) => finding.family))].sort(),
    [findings],
  );

  const shown = findings.filter((finding) => {
    if (severity && finding.severity !== severity) return false;
    if (family && finding.family !== family) return false;
    if (fixable && !finding.autofixable) return false;
    if (term) {
      const needle = term.toLowerCase();
      return (
        finding.rule.includes(needle) ||
        finding.file.toLowerCase().includes(needle) ||
        finding.evidence.toLowerCase().includes(needle)
      );
    }
    return true;
  });

  const toggle = (current, value, set) => set(current === value ? null : value);

  return html`
    <div class="page-head">
      <h1>Findings</h1>
      <div class="sub">${shown.length} of ${findings.length}</div>
    </div>

    <div class="filters">
      ${["error", "warning", "info"].map(
        (level) => html`
          <button class="chip" key=${level} aria-pressed=${severity === level}
                  onClick=${() => toggle(severity, level, setSeverity)}>${level}</button>
        `,
      )}
      <button class="chip" aria-pressed=${fixable} onClick=${() => setFixable(!fixable)}>
        auto-fixable
      </button>
      ${families.map(
        (name) => html`
          <button class="chip" key=${name} aria-pressed=${family === name}
                  onClick=${() => toggle(family, name, setFamily)}>${name}</button>
        `,
      )}
      <input class="search" placeholder="Filter by rule, path or evidence"
             value=${term} onInput=${(event) => setTerm(event.target.value)} />
    </div>

    <div class="rows">
      ${shown.slice(0, 400).map(
        (finding, index) => html`
          <button class="row row-findings" key=${`${finding.file}${finding.start}${index}`}
                  onClick=${() => openFinding(finding)}>
            <span class="pip" style="background:${SEVERITY_INK[finding.severity]}"></span>
            <span class="rule">${finding.rule}</span>
            <span class="evidence">${finding.evidence}</span>
            <span class="num" title=${finding.file}>${shortPath(finding.file)}:${finding.line}</span>
          </button>
        `,
      )}
      ${shown.length === 0 && html`<div class="empty">Nothing matches.</div>`}
    </div>
  `;
}

/* ── files ────────────────────────────────────────────────────────────── */

function Files({ report, openFile }) {
  const grades = report.grades || {};
  const impact = report.impact || {};
  const counts = {};
  for (const finding of report.findings || []) {
    counts[finding.file] = (counts[finding.file] || 0) + 1;
  }
  const order = GRADES.slice().reverse();
  const files = Object.keys(grades).sort((left, right) => {
    const rank = order.indexOf(grades[right]) - order.indexOf(grades[left]);
    return rank !== 0 ? rank : (counts[right] || 0) - (counts[left] || 0);
  });

  return html`
    <div class="page-head">
      <h1>Files</h1>
      <div class="sub">worst grade first</div>
    </div>
    <div class="rows">
      ${files.slice(0, 300).map(
        (file) => html`
          <button class="row row-files" key=${file} onClick=${() => openFile(file)}>
            <span class="path" title=${file}>${file}</span>
            <span style="color:${GRADE_INK[grades[file]]}">${grades[file]}</span>
            <span class="num">${counts[file] || 0}</span>
            <span class="num">${impact[file]?.importers || 0} in</span>
          </button>
        `,
      )}
    </div>
  `;
}

/* ── plan ─────────────────────────────────────────────────────────────── */

function Plan({ openFinding }) {
  const [budget, setBudget] = useState(12);
  const [plan, setPlan] = useState(null);

  useEffect(() => {
    let live = true;
    json(`/api/plan?budget=${budget}`).then((answer) => live && setPlan(answer));
    return () => (live = false);
  }, [budget]);

  if (!plan) return html`<div class="empty">Ranking…</div>`;

  return html`
    <div class="page-head">
      <h1>Plan</h1>
      <div class="sub">
        health ${plan.healthBefore} → <b style="color:${scoreInk(plan.healthAfter)}">${plan.healthAfter}</b>
      </div>
    </div>

    <div class="filters">
      <label class="sub" for="budget">Budget</label>
      <input id="budget" type="range" min="2" max="40" value=${budget} style="flex:1;max-width:320px"
             onInput=${(event) => setBudget(Number(event.target.value))} />
      <span class="tag">${budget} points</span>
    </div>

    <div class="rows">
      ${plan.items.map(
        (item, index) => html`
          <button class="row row-plan" key=${`${item.file}${item.line}`}
                  onClick=${() => openFinding({ ...item, evidence: "", why: "" })}>
            <span class="num">${index + 1}</span>
            <span>
              <span class="rule">${item.rule}</span>
              <span class="sub"> · ${shortPath(item.file)}:${item.line}</span>
            </span>
            <span class="evidence">${item.technique.join(" / ")}</span>
            <span class="num">${item.effort} pt</span>
            <span class="num" style="color:${scoreInk(item.healthAfter)}">${item.healthAfter}</span>
          </button>
        `,
      )}
      ${plan.items.length === 0 && html`<div class="empty">Nothing worth ranking.</div>`}
    </div>
  `;
}

/* ── rules ────────────────────────────────────────────────────────────── */

function Rules({ report }) {
  const [rules, setRules] = useState([]);
  const [term, setTerm] = useState("");

  useEffect(() => {
    json("/api/rules").then(setRules);
  }, []);

  const fired = {};
  for (const finding of report.findings || []) {
    fired[finding.rule] = (fired[finding.rule] || 0) + 1;
  }
  const shown = rules.filter(
    (rule) =>
      !term ||
      rule.slug.includes(term.toLowerCase()) ||
      rule.why.toLowerCase().includes(term.toLowerCase()),
  );
  const families = [...new Set(shown.map((rule) => rule.family))];

  return html`
    <div class="page-head">
      <h1>Rules</h1>
      <div class="sub">${rules.length} in the catalogue</div>
    </div>
    <div class="filters">
      <input class="search" placeholder="Filter the catalogue" value=${term}
             onInput=${(event) => setTerm(event.target.value)} />
    </div>
    ${families.map(
      (family) => html`
        <section key=${family}>
          <h2>${family}</h2>
          <div class="rows">
            ${shown
              .filter((rule) => rule.family === family)
              .map(
                (rule) => html`
                  <div class="row row-findings" key=${rule.slug}>
                    <span class="pip" style="background:${fired[rule.slug] ? "var(--warn)" : "var(--line-strong)"}"></span>
                    <span class="rule">${rule.slug}</span>
                    <span class="evidence">${rule.why}</span>
                    <span class="num">${fired[rule.slug] || 0}</span>
                  </div>
                `,
              )}
          </div>
        </section>
      `,
    )}
  `;
}

/* ── activity ─────────────────────────────────────────────────────────── */

function Activity({ feed }) {
  return html`
    <div class="page-head">
      <h1>Activity</h1>
      <div class="sub">every save, as it lands</div>
    </div>
    <div class="feed">
      ${feed.map(
        (entry, index) => html`
          <div class="event" key=${entry.id}>
            <span class="num">${entry.elapsedMs} ms</span>
            <span class="path" title=${entry.file}>${entry.file}</span>
            <span>
              ${entry.resolved > 0 &&
              html`<span class="tag fixable">−${entry.resolved}</span>`}
              ${entry.introduced > 0 &&
              html`<span class="tag" style="color:var(--bad)"> +${entry.introduced}</span>`}
              ${entry.resolved === 0 && entry.introduced === 0 &&
              html`<span class="tag">no change</span>`}
            </span>
          </div>
        `,
      )}
      ${feed.length === 0 &&
      html`<div class="empty">Save a file and it appears here within a second.</div>`}
    </div>
  `;
}

/* ── drawer ───────────────────────────────────────────────────────────── */

function Drawer({ target, onClose }) {
  const [source, setSource] = useState(null);

  useEffect(() => {
    if (!target) return;
    setSource(null);
    fetch(`/api/file?path=${encodeURIComponent(target.file)}`)
      .then((answer) => (answer.ok ? answer.text() : null))
      .then(setSource);
  }, [target && target.file, target && target.line]);

  useEffect(() => {
    const escape = (event) => event.key === "Escape" && onClose();
    addEventListener("keydown", escape);
    return () => removeEventListener("keydown", escape);
  }, [onClose]);

  if (!target) return null;

  const lines = (source || "").split("\n");
  const first = Math.max((target.line || 1) - 6, 1);
  const last = Math.min((target.endLine || target.line || 1) + 5, lines.length);
  const window = lines.slice(first - 1, last);

  return html`
    <div>
      <div class="scrim" onClick=${onClose}></div>
      <aside class="drawer" role="dialog" aria-label=${target.rule || target.file}>
        <div class="drawer-head">
          <div>
            <div class="rule" style="font-size:14px">${target.rule || target.file}</div>
            <div class="sub" style="font-family:var(--mono);font-size:12px">
              ${target.file}${target.line ? `:${target.line}` : ""}
            </div>
          </div>
          <button class="chip" onClick=${onClose}>Close</button>
        </div>
        <div class="drawer-body">
          ${target.evidence && html`
            <div class="field">
              <div class="field-label">Measured</div>
              <div>${target.evidence}</div>
            </div>`}
          ${target.why && html`
            <div class="field">
              <div class="field-label">Why it matters</div>
              <div>${target.why}</div>
            </div>`}
          ${target.technique && html`
            <div class="field">
              <div class="field-label">Refactoring</div>
              <div style="color:var(--good)">${target.technique.join(" / ")}</div>
              ${target.hint && html`<div class="sub">${target.hint}</div>`}
            </div>`}
          ${target.autofixable && html`
            <div class="field">
              <span class="tag fixable">rqx fix --write applies this one</span>
            </div>`}
          <div class="field">
            <div class="field-label">Source</div>
            ${source === null
              ? html`<div class="sub">Reading…</div>`
              : html`
                  <div class="code">
                    ${window.map(
                      (text, index) => html`
                        <div class="code-line" key=${first + index}
                             data-marked=${String(
                               first + index >= (target.line || 0) &&
                                 first + index <= (target.endLine || target.line || 0),
                             )}>
                          <span class="code-number">${first + index}</span>
                          <span>${text || " "}</span>
                        </div>
                      `,
                    )}
                  </div>
                `}
          </div>
        </div>
      </aside>
    </div>
  `;
}

/* ── command palette ──────────────────────────────────────────────────── */

function Palette({ report, go, openFile, onClose }) {
  const [term, setTerm] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef(null);

  useEffect(() => input.current && input.current.focus(), []);

  const options = useMemo(() => {
    const pages = PAGES.map(([id, label]) => ({
      label,
      note: "Page",
      run: () => go(id),
    }));
    const files = Object.keys(report.grades || {})
      .filter((file) => !term || file.toLowerCase().includes(term.toLowerCase()))
      .slice(0, 8)
      .map((file) => ({ label: file, note: "File", run: () => openFile(file) }));
    const matching = pages.filter(
      (page) => !term || page.label.toLowerCase().includes(term.toLowerCase()),
    );
    return [...matching, ...files];
  }, [term, report]);

  useEffect(() => {
    const key = (event) => {
      if (event.key === "Escape") onClose();
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActive((index) => Math.min(index + 1, options.length - 1));
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActive((index) => Math.max(index - 1, 0));
      }
      if (event.key === "Enter" && options[active]) {
        options[active].run();
        onClose();
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, [options, active, onClose]);

  return html`
    <div class="palette-scrim" onClick=${onClose}>
      <div class="palette" onClick=${(event) => event.stopPropagation()}>
        <input ref=${input} placeholder="Go to a page or a file…" value=${term}
               onInput=${(event) => {
                 setTerm(event.target.value);
                 setActive(0);
               }} />
        <div class="palette-list">
          ${options.map(
            (option, index) => html`
              <button class="palette-item" key=${option.label + index}
                      data-active=${String(index === active)}
                      onMouseEnter=${() => setActive(index)}
                      onClick=${() => {
                        option.run();
                        onClose();
                      }}>
                <span class="path" style="direction:ltr">${option.label}</span>
                <span class="kbd">${option.note}</span>
              </button>
            `,
          )}
          ${options.length === 0 && html`<div class="empty">Nothing matches.</div>`}
        </div>
      </div>
    </div>
  `;
}

/* ── shell ────────────────────────────────────────────────────────────── */

function App() {
  const [page, setPage] = useState(location.hash.slice(1) || "overview");
  const [report, setReport] = useState(null);
  const [trend, setTrend] = useState([]);
  const [levels, setLevels] = useState([]);
  const [feed, setFeed] = useState([]);
  const [target, setTarget] = useState(null);
  const [palette, setPalette] = useState(false);
  const [beat, setBeat] = useState(0);
  const [connected, setConnected] = useState(false);

  const load = useCallback(() => {
    json("/api/report").then(setReport);
    json("/api/trend").then(setTrend);
    json("/api/levels").then(setLevels);
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    const events = new EventSource("/events");
    events.onopen = () => setConnected(true);
    events.onerror = () => setConnected(false);
    events.onmessage = (message) => {
      const update = JSON.parse(message.data);
      load();
      setBeat((count) => count + 1);
      setFeed((current) =>
        [
          ...update.changed.map((change, index) => ({
            ...change,
            elapsedMs: update.elapsedMs,
            id: `${Date.now()}-${index}`,
          })),
          ...current,
        ].slice(0, 60),
      );
    };
    return () => events.close();
  }, [load]);

  const go = useCallback((id) => {
    setPage(id);
    location.hash = id;
  }, []);

  useEffect(() => {
    const key = (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "k") {
        event.preventDefault();
        setPalette(true);
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, []);

  if (!report) return html`<div class="booting">Reading the project…</div>`;

  const openFile = (file) => setTarget({ file, line: 1, endLine: 1 });
  const counts = {
    findings: (report.findings || []).length,
    files: Object.keys(report.grades || {}).length,
  };

  return html`
    <div class="shell">
      <nav class="rail">
        <div class="brand">
          <div class="mark">r</div>
          <div class="brand-name">${report.project || "rqx"}</div>
        </div>
        ${PAGES.map(
          ([id, label]) => html`
            <button class="nav" key=${id} aria-current=${page === id ? "page" : null}
                    onClick=${() => go(id)}>
              <span>${label}</span>
              ${counts[id] !== undefined && html`<span class="nav-count">${counts[id]}</span>`}
            </button>
          `,
        )}
        <div class="rail-foot">
          <span class="pulse">
            <span class=${`dot ${connected ? "" : "stale"} ${beat ? "beat" : ""}`} key=${beat}></span>
            ${connected ? "watching" : "reconnecting"}
          </span>
          <span>⌘K to jump</span>
        </div>
      </nav>

      <main>
        ${page === "overview" &&
        html`<${Overview} report=${report} trend=${trend} levels=${levels} go=${go} />`}
        ${page === "map" && html`<${Treemap} report=${report} openFile=${openFile} />`}
        ${page === "findings" && html`<${Findings} report=${report} openFinding=${setTarget} />`}
        ${page === "files" && html`<${Files} report=${report} openFile=${openFile} />`}
        ${page === "plan" && html`<${Plan} openFinding=${setTarget} />`}
        ${page === "rules" && html`<${Rules} report=${report} />`}
        ${page === "activity" && html`<${Activity} feed=${feed} />`}
      </main>

      <${Drawer} target=${target} onClose=${() => setTarget(null)} />
      ${palette &&
      html`<${Palette} report=${report} go=${go} openFile=${openFile}
                       onClose=${() => setPalette(false)} />`}
    </div>
  `;
}

render(h(App), document.getElementById("root"));
