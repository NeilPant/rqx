/* The dashboard. One file, no build step: the binary that serves it promises no
   runtime dependencies, and a page that needed a bundler to exist would be a second
   toolchain in a repository that has one. */

const { h, render } = preact;
const { useState, useEffect, useMemo, useRef, useCallback } = preactHooks;
const html = htm.bind(h);

const GRADES = ["pristine", "clean", "fair", "poor", "critical"];
const SEVERITIES = ["error", "warning", "info"];

/* The same bands the terminal paints: `grade_ink` and `severity_ink` in brand.rs.
   A grade that was green in `rqx scan` and amber here would be two verdicts. */
const GRADE_INK = {
  pristine: "var(--ink-good)",
  clean: "var(--ink-good)",
  fair: "var(--ink-accent)",
  poor: "var(--ink-warn)",
  critical: "var(--ink-danger)",
};
const SEVERITY_INK = {
  error: "var(--ink-danger)",
  warning: "var(--ink-warn)",
  info: "var(--ink-muted)",
};

/* The owl is rendered by the server from the same pixel art the terminal reads, and
   parked in a <template> so every copy on the page is one clone rather than one more
   hand-drawn svg. */
const OWL = document.getElementById("owl")?.innerHTML ?? "";

/* Served in the page itself, and required on every write: a form on another site can
   reach a loopback port, but it cannot read this page to learn the nonce. */
const NONCE = document.documentElement.dataset.nonce || "";

function Owl({ band, className }) {
  return h("span", {
    class: band ? `${className} owl-${band}` : className,
    dangerouslySetInnerHTML: { __html: OWL },
  });
}

const PAGES = [
  ["overview", "Overview"],
  ["map", "Map"],
  ["graph", "Graph"],
  ["findings", "Findings"],
  ["files", "Files"],
  ["plan", "Plan"],
  ["perf", "Bottlenecks"],
  ["patterns", "Patterns"],
  ["check", "Check"],
  ["rules", "Rules"],
  ["waivers", "Waivers"],
  ["config", "Config"],
  ["activity", "Activity"],
];

const json = (url) => fetch(url).then((answer) => answer.json());
const shortPath = (path) => path.split("/").slice(-2).join("/");
const directoryOf = (path) => path.split("/").slice(0, -1).join("/") || ".";

async function post(url, body) {
  const answer = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", "x-rqx-nonce": NONCE },
    body: JSON.stringify(body),
  });
  const text = await answer.text();
  if (!answer.ok) {
    throw new Error(text.trim() || `the server answered ${answer.status}`);
  }
  return text ? JSON.parse(text) : {};
}

/* Gitignore-style globs, the same spelling `--path` takes, so a filter typed here and
   one passed on the command line select the same files. */
function globMatches(pattern, path) {
  if (!pattern) return true;
  const source = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*\*\//g, "\u0000")
    .replace(/\*\*/g, "\u0001")
    .replace(/\*/g, "[^/]*")
    .replace(/\?/g, "[^/]")
    .replace(/\u0000/g, "(?:.*/)?")
    .replace(/\u0001/g, ".*");
  try {
    const anchored = new RegExp(`^${source}$`);
    if (anchored.test(path)) return true;
    // A pattern with no slash in it names a file rather than a route to one, exactly
    // as gitignore reads it.
    return !pattern.includes("/") && path.split("/").some((part) => anchored.test(part));
  } catch {
    return true;
  }
}

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

/* `score_ink` in brand.rs: 80 and 50, not 90 and 70. The page used its own cutoffs
   and disagreed with the terminal about whether the same number was healthy. */
function scoreInk(score) {
  if (score >= 80) return "var(--ink-good)";
  if (score >= 50) return "var(--ink-warn)";
  return "var(--ink-danger)";
}

/* ── a list that does not lie about its length ────────────────────────── */

/* Rows are a fixed height so the window can be computed rather than measured. The
   stylesheet pins `.rows-virtual .row` to the same number. */
const ROW_HEIGHT = 40;
const OVERSCAN = 10;

/* Only the rows in view are built. The page used to cut its lists off at four
   hundred and say nothing, so a large repository was shown a number in the header it
   could not scroll to. */
function Virtual({ items, row, empty }) {
  const [window_, setWindow] = useState({ start: 0, count: 60 });
  const anchor = useRef(null);

  useEffect(() => {
    const scroller = document.querySelector("main");
    if (!scroller) return undefined;
    const measure = () => {
      const node = anchor.current;
      if (!node) return;
      const above = Math.max(
        scroller.getBoundingClientRect().top - node.getBoundingClientRect().top,
        0,
      );
      const start = Math.max(Math.floor(above / ROW_HEIGHT) - OVERSCAN, 0);
      const count = Math.ceil(scroller.clientHeight / ROW_HEIGHT) + OVERSCAN * 2;
      setWindow((current) =>
        current.start === start && current.count === count ? current : { start, count },
      );
    };
    measure();
    scroller.addEventListener("scroll", measure, { passive: true });
    addEventListener("resize", measure);
    return () => {
      scroller.removeEventListener("scroll", measure);
      removeEventListener("resize", measure);
    };
  }, [items.length]);

  if (items.length === 0) {
    return html`<div class="empty">${empty}</div>`;
  }
  const start = Math.min(window_.start, Math.max(items.length - 1, 0));
  const shown = items.slice(start, start + window_.count);
  const below = Math.max(items.length - start - shown.length, 0);
  return html`
    <div class="rows rows-virtual" ref=${anchor}>
      ${start > 0 && html`<div style="height:${start * ROW_HEIGHT}px"></div>`}
      ${shown.map(row)}
      ${below > 0 && html`<div style="height:${below * ROW_HEIGHT}px"></div>`}
    </div>
  `;
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
         role="img" aria-label="Health over time, now ${last}">
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

function Overview({ report, trend, levels, go, act }) {
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
      <${Owl} band=${report.mascot} className="hero-owl" />
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
        <div class="card-value" style="color:var(--ink-danger)">${severities.error || 0}</div>
        <div class="card-note">never a false positive</div>
      </div>
      <div class="card">
        <div class="card-label">Warnings</div>
        <div class="card-value" style="color:var(--ink-warn)">${severities.warning || 0}</div>
      </div>
      <div class="card">
        <div class="card-label">Auto-fixable</div>
        <div class="card-value" style="color:var(--ink-good)">
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
        threshold decides whether a finding exists at all — run${" "}
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
            <button class="row row-files" key=${file.file}
                    onClick=${() => go("findings", { file: file.file })}>
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

    ${!report.readOnly &&
    html`
      <section>
        <h2>Baseline</h2>
        <div class="sub" style="margin:-6px 0 12px">
          Records every finding the project has now, so the gate judges what a change
          adds rather than what it inherited.
        </div>
        <div class="filters">
          <button class="chip" onClick=${() => act("Take the baseline", () =>
            post("/api/baseline", { update: true }).then(
              (taken) => `recorded ${taken.recorded} findings in ${taken.path}`,
            ))}>
            Take or update the baseline
          </button>
        </div>
      </section>
    `}
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

function Treemap({ report, go }) {
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
                  onClick=${() => go("findings", { file: tile.file })}
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

/* ── graph ────────────────────────────────────────────────────────────── */

/* A plain spring layout, run once when the data arrives. It is O(n²) per step, which
   is why the drawing is capped: a repository with thousands of modules is an
   unreadable hairball long before it is a slow one.
   ponytail: swap in Barnes-Hut only if a readable graph ever needs more nodes. */
const GRAPH_NODES = 300;
const GRAPH_STEPS = 260;

function layout(nodes, links, width, height) {
  const count = nodes.length;
  const positions = nodes.map((_, index) => {
    const angle = (index / Math.max(count, 1)) * Math.PI * 2;
    return {
      x: width / 2 + Math.cos(angle) * width * 0.35,
      y: height / 2 + Math.sin(angle) * height * 0.35,
    };
  });
  const area = (width * height) / Math.max(count, 1);
  const ideal = Math.sqrt(area) * 0.7;

  for (let step = 0; step < GRAPH_STEPS; step += 1) {
    const cooling = 1 - step / GRAPH_STEPS;
    const shifts = positions.map(() => ({ x: 0, y: 0 }));
    for (let a = 0; a < count; a += 1) {
      for (let b = a + 1; b < count; b += 1) {
        let dx = positions[a].x - positions[b].x;
        let dy = positions[a].y - positions[b].y;
        let distance = Math.hypot(dx, dy) || 0.01;
        // Two nodes at the same point have no direction to push apart in, so one is
        // nudged off the other rather than dividing by zero forever.
        if (distance < 0.05) {
          dx = (a % 2 ? 1 : -1) * 0.05;
          dy = 0.05;
          distance = 0.07;
        }
        const push = (ideal * ideal) / distance;
        shifts[a].x += (dx / distance) * push;
        shifts[a].y += (dy / distance) * push;
        shifts[b].x -= (dx / distance) * push;
        shifts[b].y -= (dy / distance) * push;
      }
    }
    for (const [from, to] of links) {
      const dx = positions[from].x - positions[to].x;
      const dy = positions[from].y - positions[to].y;
      const distance = Math.hypot(dx, dy) || 0.01;
      const pull = (distance * distance) / ideal;
      shifts[from].x -= (dx / distance) * pull;
      shifts[from].y -= (dy / distance) * pull;
      shifts[to].x += (dx / distance) * pull;
      shifts[to].y += (dy / distance) * pull;
    }
    for (let index = 0; index < count; index += 1) {
      const shift = shifts[index];
      const length = Math.hypot(shift.x, shift.y) || 1;
      const step_ = Math.min(length, ideal * cooling);
      positions[index].x = Math.max(
        12,
        Math.min(width - 12, positions[index].x + (shift.x / length) * step_),
      );
      positions[index].y = Math.max(
        12,
        Math.min(height - 12, positions[index].y + (shift.y / length) * step_),
      );
    }
  }
  return positions;
}

function Graph({ go }) {
  const [graph, setGraph] = useState(null);
  const [cyclesOnly, setCyclesOnly] = useState(false);
  const [hover, setHover] = useState(null);

  useEffect(() => {
    json("/api/graph").then(setGraph);
  }, []);

  const drawing = useMemo(() => {
    if (!graph) return null;
    const width = 960;
    const height = 620;
    const inCycle = new Set((graph.cycles || []).flat());
    const degree = new Map();
    const bump = (file) => degree.set(file, (degree.get(file) || 0) + 1);
    for (const [importer, imported] of Object.entries(graph.edges || {})) {
      for (const target of imported) {
        bump(importer);
        bump(target);
      }
    }
    // A cycle is the reason to open this page, so its members are never the ones a
    // cap drops.
    const candidates = (graph.nodes || [])
      .filter((node) => (cyclesOnly ? inCycle.has(node.file) : degree.has(node.file)))
      .sort((left, right) => {
        const cycle = Number(inCycle.has(right.file)) - Number(inCycle.has(left.file));
        if (cycle !== 0) return cycle;
        return (degree.get(right.file) || 0) - (degree.get(left.file) || 0);
      });
    const nodes = candidates.slice(0, GRAPH_NODES);
    const index = new Map(nodes.map((node, at) => [node.file, at]));
    const links = [];
    for (const [importer, imported] of Object.entries(graph.edges || {})) {
      const from = index.get(importer);
      if (from === undefined) continue;
      for (const target of imported) {
        const to = index.get(target);
        if (to !== undefined && to !== from) links.push([from, to]);
      }
    }
    const cyclic = new Set();
    for (const members of graph.cycles || []) {
      for (const one of members) {
        for (const other of members) {
          cyclic.add(`${index.get(one)}-${index.get(other)}`);
        }
      }
    }
    return {
      width,
      height,
      nodes,
      links,
      inCycle,
      cyclic,
      positions: layout(nodes, links, width, height),
      dropped: candidates.length - nodes.length,
    };
  }, [graph, cyclesOnly]);

  if (!drawing) return html`<div class="empty">Reading the import graph…</div>`;

  const radius = (node) => Math.max(4, Math.min(16, Math.sqrt(node.lines || 1) / 2));

  return html`
    <div class="page-head">
      <h1>Graph</h1>
      <div class="sub">
        ${drawing.nodes.length} modules · ${(graph.cycles || []).length} cycles
        ${drawing.dropped > 0 ? ` · ${drawing.dropped} less-connected modules not drawn` : ""}
      </div>
    </div>
    <div class="filters">
      <button class="chip" aria-pressed=${cyclesOnly} onClick=${() => setCyclesOnly(!cyclesOnly)}>
        cycles only
      </button>
      <span class="sub">${hover || "an edge points from the importer to what it imports"}</span>
    </div>
    <div class="graph">
      <svg viewBox="0 0 ${drawing.width} ${drawing.height}" role="img"
           aria-label="The import graph, cycles in red">
        <g>
          ${drawing.links.map(([from, to], index) => {
            const cyclic = drawing.cyclic.has(`${from}-${to}`);
            return html`
              <line key=${index}
                    x1=${drawing.positions[from].x} y1=${drawing.positions[from].y}
                    x2=${drawing.positions[to].x} y2=${drawing.positions[to].y}
                    stroke=${cyclic ? "var(--ink-danger)" : "var(--border)"}
                    stroke-width=${cyclic ? 1.6 : 0.7} />
            `;
          })}
        </g>
        <g>
          ${drawing.nodes.map(
            (node, index) => html`
              <circle key=${node.file}
                      cx=${drawing.positions[index].x} cy=${drawing.positions[index].y}
                      r=${radius(node)}
                      fill=${GRADE_INK[node.grade]}
                      stroke=${drawing.inCycle.has(node.file) ? "var(--ink-danger)" : "transparent"}
                      stroke-width="2"
                      class="node"
                      onMouseEnter=${() =>
                        setHover(`${node.file} · ${node.grade} · ${node.importers} importers`)}
                      onMouseLeave=${() => setHover(null)}
                      onClick=${() => go("findings", { file: node.file })}>
                <title>${node.file}</title>
              </circle>
            `,
          )}
        </g>
      </svg>
    </div>
    ${(graph.cycles || []).length > 0 &&
    html`
      <section>
        <h2>Cycles</h2>
        <div class="rows">
          ${graph.cycles.map(
            (members, index) => html`
              <button class="row" style="grid-template-columns:1fr" key=${index}
                      onClick=${() => go("findings", { file: members[0] })}>
                <span class="path">${members.join("  →  ")}</span>
              </button>
            `,
          )}
        </div>
      </section>
    `}
  `;
}

/* ── findings ─────────────────────────────────────────────────────────── */

const SORTS = [
  ["rank", "rank"],
  ["severity", "severity"],
  ["rule", "rule"],
  ["file", "file"],
];
const GROUPS = [
  ["none", "flat"],
  ["file", "by file"],
  ["rule", "by rule"],
  ["family", "by family"],
  ["directory", "by directory"],
];

/* Everything the reader chose lives in the address, so a filtered view can be
   reloaded, bookmarked and sent to somebody else. */
function Findings({ report, params, go, openFinding }) {
  const set = (key, value) => {
    const next = { ...params };
    if (value === null || value === "" || value === false) delete next[key];
    else next[key] = String(value);
    go("findings", next);
  };
  const toggle = (key, value) => set(key, params[key] === value ? null : value);

  const findings = report.findings || [];
  const grades = report.grades || {};
  const families = useMemo(
    () => [...new Set(findings.map((finding) => finding.family))].sort(),
    [findings],
  );
  const rules = useMemo(
    () => [...new Set(findings.map((finding) => finding.rule))].sort(),
    [findings],
  );

  const term = (params.q || "").toLowerCase();
  const shown = useMemo(() => {
    const kept = findings.filter((finding) => {
      if (params.severity && finding.severity !== params.severity) return false;
      if (params.family && finding.family !== params.family) return false;
      if (params.rule && finding.rule !== params.rule) return false;
      if (params.confidence && finding.confidence !== params.confidence) return false;
      if (params.grade && grades[finding.file] !== params.grade) return false;
      if (params.fixable === "1" && !finding.autofixable) return false;
      if (params.hideKnown === "1" && finding.known) return false;
      if (params.file && finding.file !== params.file) return false;
      if (params.path && !globMatches(params.path, finding.file)) return false;
      if (term) {
        return (
          finding.rule.includes(term) ||
          finding.file.toLowerCase().includes(term) ||
          (finding.evidence || "").toLowerCase().includes(term)
        );
      }
      return true;
    });
    const sort = params.sort || "rank";
    if (sort === "severity") {
      kept.sort(
        (left, right) =>
          SEVERITIES.indexOf(left.severity) - SEVERITIES.indexOf(right.severity),
      );
    } else if (sort === "rule") {
      kept.sort((left, right) => left.rule.localeCompare(right.rule));
    } else if (sort === "file") {
      kept.sort(
        (left, right) => left.file.localeCompare(right.file) || left.line - right.line,
      );
    }
    return kept;
  }, [findings, params, term, grades]);

  const row = (finding) => html`
    <button class="row row-findings" key=${`${finding.file}:${finding.start}:${finding.rule}`}
            onClick=${() => openFinding(finding)}>
      <span class="pip" style="background:${SEVERITY_INK[finding.severity]}"></span>
      <span class="rule">${finding.rule}</span>
      <span class="evidence">${finding.evidence}</span>
      <span class="num" title=${finding.file}>${shortPath(finding.file)}:${finding.line}</span>
    </button>
  `;

  const grouping = params.group || "none";
  const active = Object.keys(params).filter((key) => key !== "group" && key !== "sort");

  return html`
    <div class="page-head">
      <h1>Findings</h1>
      <div class="sub">${shown.length} of ${findings.length}</div>
    </div>

    <div class="filters">
      ${SEVERITIES.map(
        (level) => html`
          <button class="chip" key=${level} aria-pressed=${params.severity === level}
                  onClick=${() => toggle("severity", level)}>${level}</button>
        `,
      )}
      <button class="chip" aria-pressed=${params.fixable === "1"}
              onClick=${() => set("fixable", params.fixable === "1" ? null : "1")}>
        auto-fixable
      </button>
      <button class="chip" aria-pressed=${params.hideKnown === "1"}
              onClick=${() => set("hideKnown", params.hideKnown === "1" ? null : "1")}>
        new only
      </button>
      ${["measured threshold", "low"].map(
        (level) => html`
          <button class="chip" key=${level} aria-pressed=${params.confidence === level}
                  onClick=${() => toggle("confidence", level)}>${level}</button>
        `,
      )}
      ${active.length > 0 &&
      html`<button class="chip" onClick=${() => go("findings", {
        group: params.group,
        sort: params.sort,
      })}>clear</button>`}
    </div>

    <div class="filters">
      <select class="select" value=${params.rule || ""}
              onChange=${(event) => set("rule", event.target.value || null)}>
        <option value="">every rule</option>
        ${rules.map((rule) => html`<option key=${rule} value=${rule}>${rule}</option>`)}
      </select>
      <select class="select" value=${params.family || ""}
              onChange=${(event) => set("family", event.target.value || null)}>
        <option value="">every family</option>
        ${families.map((name) => html`<option key=${name} value=${name}>${name}</option>`)}
      </select>
      <select class="select" value=${params.grade || ""}
              onChange=${(event) => set("grade", event.target.value || null)}>
        <option value="">every grade</option>
        ${GRADES.map((grade) => html`<option key=${grade} value=${grade}>${grade}</option>`)}
      </select>
      <select class="select" value=${params.sort || "rank"}
              onChange=${(event) => set("sort", event.target.value)}>
        ${SORTS.map(([id, label]) => html`<option key=${id} value=${id}>sort: ${label}</option>`)}
      </select>
      <select class="select" value=${grouping}
              onChange=${(event) => set("group", event.target.value)}>
        ${GROUPS.map(([id, label]) => html`<option key=${id} value=${id}>${label}</option>`)}
      </select>
    </div>

    <div class="filters">
      <input class="search" placeholder="Filter by rule, path or evidence"
             value=${params.q || ""} onInput=${(event) => set("q", event.target.value)} />
      <input class="search" style="max-width:260px" placeholder="Path glob, e.g. src/**"
             value=${params.path || ""} onInput=${(event) => set("path", event.target.value)} />
    </div>

    ${params.file &&
    html`<div class="filters"><span class="tag">${params.file}</span>
      <button class="chip" onClick=${() => set("file", null)}>every file</button></div>`}

    ${grouping === "none"
      ? html`<${Virtual} items=${shown} row=${row} empty="Nothing matches." />`
      : html`<${Grouped} items=${shown} by=${grouping} row=${row} />`}
  `;
}

function groupKey(finding, by) {
  if (by === "file") return finding.file;
  if (by === "rule") return finding.rule;
  if (by === "family") return finding.family;
  return directoryOf(finding.file);
}

/* Groups start open on a project small enough to read at once and closed on one that
   is not, because a thousand open groups is the flat list with headings in it. */
const OPEN_BY_DEFAULT = 300;

function Grouped({ items, by, row }) {
  const groups = useMemo(() => {
    const map = new Map();
    for (const item of items) {
      const key = groupKey(item, by);
      if (!map.has(key)) map.set(key, []);
      map.get(key).push(item);
    }
    return [...map.entries()].sort((left, right) => right[1].length - left[1].length);
  }, [items, by]);
  const [closed, setClosed] = useState(new Set());
  const openByDefault = items.length <= OPEN_BY_DEFAULT;

  if (groups.length === 0) return html`<div class="empty">Nothing matches.</div>`;

  return html`
    <div class="groups">
      ${groups.map(([key, members]) => {
        const open = closed.has(key) ? !openByDefault : openByDefault;
        return html`
          <section class="group" key=${key}>
            <button class="group-head" aria-expanded=${String(open)}
                    onClick=${() =>
                      setClosed((current) => {
                        const next = new Set(current);
                        if (next.has(key)) next.delete(key);
                        else next.add(key);
                        return next;
                      })}>
              <span class="group-caret">${open ? "▾" : "▸"}</span>
              <span class="path">${key}</span>
              <span class="num">${members.length}</span>
            </button>
            ${open && html`<div class="rows">${members.map(row)}</div>`}
          </section>
        `;
      })}
    </div>
  `;
}

/* ── files ────────────────────────────────────────────────────────────── */

const FILE_SORTS = [
  ["grade", "worst grade"],
  ["findings", "most findings"],
  ["lines", "longest"],
  ["importers", "most depended on"],
  ["path", "path"],
];

function Files({ report, params, go }) {
  const set = (key, value) => {
    const next = { ...params };
    if (!value) delete next[key];
    else next[key] = String(value);
    go("files", next);
  };
  const grades = report.grades || {};
  const impact = report.impact || {};
  const lines = report.lines || {};
  const counts = {};
  for (const finding of report.findings || []) {
    counts[finding.file] = (counts[finding.file] || 0) + 1;
  }
  const term = (params.q || "").toLowerCase();
  const sort = params.sort || "grade";

  const files = useMemo(() => {
    const kept = Object.keys(grades).filter((file) => !term || file.toLowerCase().includes(term));
    // GRADES runs best to worst, so the worst-first order is its index descending.
    const by = {
      grade: (left, right) =>
        GRADES.indexOf(grades[right]) - GRADES.indexOf(grades[left]) ||
        (counts[right] || 0) - (counts[left] || 0),
      findings: (left, right) => (counts[right] || 0) - (counts[left] || 0),
      lines: (left, right) => (lines[right] || 0) - (lines[left] || 0),
      importers: (left, right) =>
        (impact[right]?.importers || 0) - (impact[left]?.importers || 0),
      path: (left, right) => left.localeCompare(right),
    };
    return kept.sort(by[sort] || by.grade);
  }, [grades, counts, lines, impact, term, sort]);

  const row = (file) => html`
    <button class="row row-files" key=${file} onClick=${() => go("findings", { file })}>
      <span class="path" title=${file}>${file}</span>
      <span style="color:${GRADE_INK[grades[file]]}">${grades[file]}</span>
      <span class="num">${counts[file] || 0}</span>
      <span class="num">${impact[file]?.importers || 0} in</span>
    </button>
  `;

  return html`
    <div class="page-head">
      <h1>Files</h1>
      <div class="sub">${files.length} of ${Object.keys(grades).length}</div>
    </div>
    <div class="filters">
      <input class="search" placeholder="Filter by path" value=${params.q || ""}
             onInput=${(event) => set("q", event.target.value)} />
      <select class="select" value=${sort} onChange=${(event) => set("sort", event.target.value)}>
        ${FILE_SORTS.map(([id, label]) => html`<option key=${id} value=${id}>sort: ${label}</option>`)}
      </select>
    </div>
    <${Virtual} items=${files} row=${row} empty="No file matches." />
  `;
}

/* ── plan ─────────────────────────────────────────────────────────────── */

function Plan({ openFinding }) {
  const [budget, setBudget] = useState(12);
  const [plan, setPlan] = useState(null);

  useEffect(() => {
    let live = true;
    json(`/api/plan?budget=${budget}`).then((answer) => live && setPlan(answer));
    return () => {
      live = false;
    };
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

/* ── bottlenecks ──────────────────────────────────────────────────────── */

function share(sampled) {
  if (!sampled) return "–";
  return `${(sampled.inclusive / 10).toFixed(1)}%`;
}

function Perf({ openFinding }) {
  const [rows, setRows] = useState(null);

  useEffect(() => {
    let live = true;
    json("/api/perf?limit=40").then((answer) => live && setRows(answer));
    return () => {
      live = false;
    };
  }, []);

  if (!rows) return html`<div class="empty">Measuring…</div>`;

  const profile = rows.profile;
  return html`
    <div class="page-head">
      <h1>Bottlenecks</h1>
      <div class="sub">
        ${rows.functions.length} worth a look${profile
          ? ` · profile: ${profile.matched} matched, ${profile.unmatched} unplaced`
          : ""}
      </div>
    </div>

    <div class="rows">
      ${rows.functions.map(
        (row) => html`
          <div class="row row-plan" key=${row.at}>
            <span class="tag">${row.order}</span>
            <span>
              <span class="rule">${row.name}</span>
              <span class="sub"> · ${shortPath(row.at)}</span>
            </span>
            <span class="evidence">
              ${row.loops} loops · ${row.io} io · ×${row.reach.toFixed(1)}
            </span>
            <span class="num">${profile ? share(row.sampled) : ""}</span>
          </div>
          ${row.findings.map(
            (finding) => html`
              <button class="row" key=${`${row.at}${finding.rule}`}
                      onClick=${() =>
                        openFinding({
                          file: row.at.split(":")[0],
                          line: Number(row.at.split(":").pop()),
                          rule: finding.rule,
                          evidence: finding.evidence,
                          why: "",
                        })}>
                <span class="num"></span>
                <span><span class="rule">${finding.rule}</span></span>
                <span class="evidence">${finding.evidence}</span>
                <span class="sub">${finding.fix}</span>
              </button>
            `,
          )}
        `,
      )}
      ${rows.functions.length === 0 &&
      html`<div class="empty">Nothing here grows faster than its input.</div>`}
    </div>
  `;
}

/* ── patterns ─────────────────────────────────────────────────────────── */

function Patterns({ report, openFinding }) {
  const advice = report.advice || [];
  const groups = useMemo(() => {
    const map = new Map();
    for (const one of advice) {
      if (!map.has(one.advisory)) map.set(one.advisory, []);
      map.get(one.advisory).push(one);
    }
    return [...map.entries()].sort((left, right) => right[1].length - left[1].length);
  }, [advice]);

  return html`
    <div class="page-head">
      <h1>Patterns</h1>
      <div class="sub">${advice.length} advisories · they never move the health score</div>
    </div>
    ${groups.length === 0 &&
    html`<div class="empty">No design pattern is half applied here.</div>`}
    ${groups.map(
      ([advisory, members]) => html`
        <section key=${advisory}>
          <h2>${advisory} <span class="sub">· ${members.length}</span></h2>
          <div class="rows">
            ${members.map(
              (one, index) => html`
                <button class="row row-findings" key=${`${one.file}${one.line}${index}`}
                        onClick=${() =>
                          openFinding({
                            rule: one.advisory,
                            file: one.file,
                            line: one.line,
                            endLine: one.line,
                            evidence: one.note,
                          })}>
                  <span class="pip" style="background:var(--ink-accent)"></span>
                  <span class="rule">${one.advisory}</span>
                  <span class="evidence">${one.note}</span>
                  <span class="num" title=${one.file}>${shortPath(one.file)}:${one.line}</span>
                </button>
              `,
            )}
          </div>
        </section>
      `,
    )}
  `;
}

/* ── check ────────────────────────────────────────────────────────────── */

function Check({ report, openFinding }) {
  const [base, setBase] = useState("origin/HEAD");
  const [verdict, setVerdict] = useState(null);
  const [running, setRunning] = useState(false);
  const [failed, setFailed] = useState(null);

  const run = () => {
    setRunning(true);
    setFailed(null);
    post("/api/check", { base })
      .then(setVerdict)
      .catch((error) => setFailed(error.message))
      .finally(() => setRunning(false));
  };

  return html`
    <div class="page-head">
      <h1>Check</h1>
      <div class="sub">what this branch added, judged against the gate</div>
    </div>
    <div class="filters">
      <input class="search" style="max-width:280px" value=${base}
             onInput=${(event) => setBase(event.target.value)} placeholder="origin/HEAD" />
      <button class="chip" onClick=${run} disabled=${running}>
        ${running ? "Reviewing…" : "Review the diff"}
      </button>
    </div>
    ${failed && html`<div class="notice bad">${failed}</div>`}
    ${verdict &&
    html`
      <div class="notice ${verdict.passed ? "good" : "bad"}">
        ${verdict.reviewedFindings} findings across ${verdict.reviewedFiles} files against
        ${" "}<b>${verdict.base}</b> — ${verdict.passed ? "passes the gate" : "fails the gate"}.
        ${verdict.known > 0 && ` ${verdict.known} already in the baseline.`}
      </div>
      ${verdict.failures.length > 0 &&
      html`
        <section>
          <h2>Why it fails</h2>
          <div class="rows">
            ${verdict.failures.map(
              (failure, index) => html`
                <div class="row" style="grid-template-columns:1fr" key=${index}>
                  <span>${failure}</span>
                </div>
              `,
            )}
          </div>
        </section>
      `}
      <section>
        <h2>Reviewed</h2>
        <div class="rows">
          ${(verdict.findings || []).map(
            (finding, index) => html`
              <button class="row row-findings" key=${`${finding.file}${index}`}
                      onClick=${() => openFinding(finding)}>
                <span class="pip" style="background:${SEVERITY_INK[finding.severity]}"></span>
                <span class="rule">${finding.rule}</span>
                <span class="evidence">${finding.evidence}</span>
                <span class="num">${shortPath(finding.file)}:${finding.line}</span>
              </button>
            `,
          )}
          ${(verdict.findings || []).length === 0 &&
          html`<div class="empty">Nothing to report in the diff.</div>`}
        </div>
      </section>
    `}
    ${!verdict && !failed && html`<div class="empty">Name a base and review the diff.</div>`}
  `;
}

/* ── rules ────────────────────────────────────────────────────────────── */

function Rules({ report, go, act }) {
  const [rules, setRules] = useState([]);
  const [term, setTerm] = useState("");

  useEffect(() => {
    json("/api/rules").then(setRules);
  }, []);

  const fired = {};
  for (const finding of report.findings || []) {
    fired[finding.rule] = (fired[finding.rule] || 0) + 1;
  }
  const needle = term.toLowerCase();
  const shown = rules.filter(
    (rule) => !term || rule.slug.includes(needle) || rule.why.toLowerCase().includes(needle),
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
                  <div class="row row-rules" key=${rule.slug}>
                    <span class="pip" style="background:${fired[rule.slug] ? "var(--ink-warn)" : "var(--border)"}"></span>
                    <span class="rule">${rule.slug}</span>
                    <span class="evidence">${rule.why}</span>
                    <button class="num link" title="Show what fired"
                            onClick=${() => go("findings", { rule: rule.slug })}>
                      ${fired[rule.slug] || 0}
                    </button>
                    ${!report.readOnly &&
                    html`
                      <button class="chip small"
                              onClick=${() =>
                                act(`Disable ${rule.slug}`, () =>
                                  post("/api/disable", { name: rule.slug }).then(
                                    () => `${rule.slug} disabled in RQX.toml`,
                                  ))}>
                        disable
                      </button>
                    `}
                  </div>
                `,
              )}
          </div>
        </section>
      `,
    )}
  `;
}

/* ── waivers ──────────────────────────────────────────────────────────── */

function Waivers({ go }) {
  const [waivers, setWaivers] = useState(null);

  useEffect(() => {
    json("/api/suppressions").then(setWaivers);
  }, []);

  if (!waivers) return html`<div class="empty">Reading the waivers…</div>`;

  return html`
    <div class="page-head">
      <h1>Waivers</h1>
      <div class="sub">${waivers.length} findings a comment asked rqx to leave out</div>
    </div>
    <div class="rows">
      ${waivers.map(
        (waiver, index) => html`
          <button class="row row-waivers" key=${`${waiver.file}${index}`}
                  onClick=${() => go("findings", { file: waiver.file })}>
            <span class="pip" style="background:${SEVERITY_INK[waiver.severity]}"></span>
            <span class="rule">${waiver.rule}</span>
            <span class="evidence">${waiver.reason || "no reason given"}</span>
            <span class="num">${waiver.until ? `until ${waiver.until}` : ""}</span>
            <span class="num" title=${waiver.file}>${shortPath(waiver.file)}:${waiver.line}</span>
          </button>
        `,
      )}
      ${waivers.length === 0 && html`<div class="empty">Nothing is waived.</div>`}
    </div>
  `;
}

/* ── config ───────────────────────────────────────────────────────────── */

function Config() {
  const [config, setConfig] = useState(null);

  useEffect(() => {
    json("/api/config").then(setConfig);
  }, []);

  if (!config) return html`<div class="empty">Reading the settings…</div>`;

  const tables = [...new Set(config.settings.map((row) => row.table))];

  return html`
    <div class="page-head">
      <h1>Config</h1>
      <div class="sub">
        level ${config.level} · ${config.sources.length
          ? config.sources.join(", ")
          : "no config file; every value is a default"}
      </div>
    </div>
    ${tables.map(
      (table) => html`
        <section key=${table}>
          <h2>${table}</h2>
          <div class="rows">
            ${config.settings
              .filter((row) => row.table === table)
              .map(
                (row) => html`
                  <div class="row row-config" key=${`${table}.${row.name}`}
                       data-default=${String(row.isDefault)}>
                    <span class="rule">${row.name}</span>
                    <span class="evidence">${row.value}</span>
                    <span class="num">${row.source}</span>
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
        (entry) => html`
          <div class="event" key=${entry.id}>
            <span class="num">${entry.elapsedMs} ms</span>
            <span class="path" title=${entry.file}>${entry.file}</span>
            <span>
              ${entry.resolved > 0 && html`<span class="tag fixable">−${entry.resolved}</span>`}
              ${entry.introduced > 0 && html`<span class="tag bad">+${entry.introduced}</span>`}
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

function editorLink(report, file, line, column) {
  if (!report.editorUrl) return null;
  const absolute = report.root ? `${report.root}/${file}` : file;
  return report.editorUrl
    .replace("{path}", absolute)
    .replace("{line}", String(line || 1))
    .replace("{column}", String(column || 1));
}

function Drawer({ target, report, onClose, act }) {
  const [source, setSource] = useState(null);
  const [preview, setPreview] = useState(null);
  const [waiving, setWaiving] = useState(false);
  const [reason, setReason] = useState("");
  const [until, setUntil] = useState("");

  useEffect(() => {
    if (!target) return;
    setSource(null);
    setPreview(null);
    setWaiving(false);
    setReason("");
    setUntil("");
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
  const shown = lines.slice(first - 1, last);
  const link = editorLink(report, target.file, target.line, target.column);
  const canAct = !report.readOnly && target.rule && target.line;

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
          <div class="filters">
            ${link && html`<a class="chip" href=${link}>Open in editor</a>`}
            ${canAct && target.autofixable &&
            html`
              <button class="chip" onClick=${() =>
                post("/api/fix", { file: target.file, write: false })
                  .then(setPreview)
                  .catch((error) => setPreview({ error: error.message }))}>
                Preview the fix
              </button>
            `}
            ${canAct &&
            html`<button class="chip" onClick=${() => setWaiving(!waiving)}>Waive…</button>`}
            ${canAct &&
            html`
              <button class="chip" onClick=${() =>
                act(`Disable ${target.rule}`, () =>
                  post("/api/disable", { name: target.rule }).then(
                    () => `${target.rule} disabled in RQX.toml`,
                  ))}>
                Disable the rule
              </button>
            `}
          </div>

          ${waiving &&
          html`
            <div class="field">
              <div class="field-label">Why this one stays</div>
              <input class="search" placeholder="rqx will not honour a waiver without a reason"
                     value=${reason} onInput=${(event) => setReason(event.target.value)} />
              <div class="filters" style="margin-top:8px">
                <input class="search" style="max-width:180px" placeholder="until YYYY-MM-DD"
                       value=${until} onInput=${(event) => setUntil(event.target.value)} />
                <button class="chip" disabled=${!reason.trim()}
                        onClick=${() =>
                          act("Waive the finding", () =>
                            post("/api/suppress", {
                              file: target.file,
                              line: target.line,
                              rule: target.rule,
                              reason,
                              until: until.trim() || null,
                            }).then(() => {
                              setWaiving(false);
                              return `waived ${target.rule} in ${target.file}`;
                            }))}>
                  Write the waiver
                </button>
              </div>
            </div>
          `}

          ${preview && html`<${FixPreview} preview=${preview} file=${target.file} act=${act} />`}

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
              <div style="color:var(--ink-good)">${target.technique.join(" / ")}</div>
              ${target.hint && html`<div class="sub">${target.hint}</div>`}
            </div>`}
          <div class="field">
            <div class="field-label">Source</div>
            ${source === null
              ? html`<div class="sub">Reading…</div>`
              : html`
                  <div class="code">
                    ${shown.map(
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

/* The edits the fixer proved, before any of them are written. Showing what would be
   removed and what replaces it is the whole question a reader has before saying yes. */
function FixPreview({ preview, file, act }) {
  if (preview.error) {
    return html`<div class="notice bad">${preview.error}</div>`;
  }
  const edits = (preview.fixes || []).flatMap((fix) => fix.edits || []);
  const withheld = preview.withheld || [];
  if (edits.length === 0) {
    return html`
      <div class="notice">
        Nothing here can be fixed mechanically.
        ${withheld.length > 0 && ` Withheld: ${withheld.map((one) => one.reason).join("; ")}`}
      </div>
    `;
  }
  return html`
    <div class="field">
      <div class="field-label">${edits.length} proven edits</div>
      <div class="code">
        ${edits.map(
          (edit, index) => html`
            <div class="code-line" key=${index}>
              <span class="code-number">${edit.line}</span>
              <span>
                <span class="rule">${edit.rule}</span>${" "}
                <s style="color:var(--ink-danger)">${(edit.removed || "").trim() || "∅"}</s>${" → "}
                <span style="color:var(--ink-good)">${(edit.replacement || "").trim() || "∅"}</span>
              </span>
            </div>
          `,
        )}
      </div>
      <div class="filters" style="margin-top:10px">
        <button class="chip" onClick=${() =>
          act("Apply the fix", () =>
            post("/api/fix", { file, write: true }).then(
              (written) => `applied ${written.summary.applied} edits`,
            ))}>
          Apply to ${file.split("/").pop()}
        </button>
      </div>
    </div>
  `;
}

/* ── command palette ──────────────────────────────────────────────────── */

function Palette({ report, go, onClose }) {
  const [term, setTerm] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef(null);

  useEffect(() => input.current && input.current.focus(), []);

  const options = useMemo(() => {
    const needle = term.toLowerCase();
    const pages = PAGES.filter(([, label]) => !term || label.toLowerCase().includes(needle)).map(
      ([id, label]) => ({ label, note: "Page", run: () => go(id) }),
    );
    const files = Object.keys(report.grades || {})
      .filter((file) => term && file.toLowerCase().includes(needle))
      .slice(0, 8)
      .map((file) => ({
        label: file,
        note: "File",
        run: () => go("findings", { file }),
      }));
    const rules = [...new Set((report.findings || []).map((finding) => finding.rule))]
      .filter((rule) => term && rule.includes(needle))
      .slice(0, 6)
      .map((rule) => ({ label: rule, note: "Rule", run: () => go("findings", { rule }) }));
    return [...pages, ...files, ...rules];
  }, [term, report, go]);

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
        <input ref=${input} placeholder="Go to a page, a file or a rule…" value=${term}
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

/* The whole view is in the address: the page, and everything the reader filtered,
   sorted or grouped by. Back, forward and a pasted link all land where they should. */
function readRoute() {
  const raw = decodeURIComponent(location.hash.slice(1));
  const cut = raw.indexOf("?");
  const page = (cut === -1 ? raw : raw.slice(0, cut)) || "overview";
  const params = {};
  if (cut !== -1) {
    for (const [key, value] of new URLSearchParams(raw.slice(cut + 1))) {
      params[key] = value;
    }
  }
  return { page, params };
}

function writeRoute(page, params) {
  const query = new URLSearchParams(
    Object.entries(params || {}).filter(([, value]) => value !== undefined && value !== ""),
  ).toString();
  location.hash = query ? `${page}?${query}` : page;
}

function App() {
  const [route, setRoute] = useState(readRoute);
  const [report, setReport] = useState(null);
  const [trend, setTrend] = useState([]);
  const [levels, setLevels] = useState([]);
  const [feed, setFeed] = useState([]);
  const [target, setTarget] = useState(null);
  const [palette, setPalette] = useState(false);
  const [beat, setBeat] = useState(0);
  const [connected, setConnected] = useState(false);
  const [notice, setNotice] = useState(null);

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

  const go = useCallback((page, params) => {
    // A drawer left open over a page the reader has moved away from is a drawer they
    // have to dismiss before they can see what they asked for.
    setTarget(null);
    writeRoute(page, params);
    setRoute({ page, params: params || {} });
  }, []);

  useEffect(() => {
    const moved = () => {
      setRoute(readRoute());
      setTarget(null);
    };
    addEventListener("hashchange", moved);
    return () => removeEventListener("hashchange", moved);
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

  /* Every write goes through here, so one place says what is happening, what came
     back, and what refused — rather than each button inventing its own answer. */
  const act = useCallback(
    (label, run) => {
      setNotice({ label, state: "running", message: "working…" });
      run()
        .then((message) => setNotice({ label, state: "good", message }))
        .catch((error) => setNotice({ label, state: "bad", message: error.message }));
    },
    [],
  );

  useEffect(() => {
    if (!notice || notice.state === "running") return undefined;
    const timer = setTimeout(() => setNotice(null), 6000);
    return () => clearTimeout(timer);
  }, [notice]);

  if (!report) return html`<div class="booting">Reading the project…</div>`;

  const { page, params } = route;
  const counts = {
    findings: (report.findings || []).length,
    files: Object.keys(report.grades || {}).length,
    patterns: (report.advice || []).length,
    waivers: report.suppressed || 0,
  };

  return html`
    <div class="shell">
      <nav class="rail">
        <div class="brand">
          <${Owl} band=${report.mascot} className="mark" />
          <div>
            <div class="brand-name">${report.project || "rqx"}</div>
            <div class="brand-sub">health ${report.health ?? "—"}/100</div>
          </div>
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
        html`<${Overview} report=${report} trend=${trend} levels=${levels} go=${go} act=${act} />`}
        ${page === "map" && html`<${Treemap} report=${report} go=${go} />`}
        ${page === "graph" && html`<${Graph} go=${go} />`}
        ${page === "findings" &&
        html`<${Findings} report=${report} params=${params} go=${go} openFinding=${setTarget} />`}
        ${page === "files" && html`<${Files} report=${report} params=${params} go=${go} />`}
        ${page === "plan" && html`<${Plan} openFinding=${setTarget} />`}
        ${page === "perf" && html`<${Perf} openFinding=${setTarget} />`}
        ${page === "patterns" && html`<${Patterns} report=${report} openFinding=${setTarget} />`}
        ${page === "check" && html`<${Check} report=${report} openFinding=${setTarget} />`}
        ${page === "rules" && html`<${Rules} report=${report} go=${go} act=${act} />`}
        ${page === "waivers" && html`<${Waivers} go=${go} />`}
        ${page === "config" && html`<${Config} />`}
        ${page === "activity" && html`<${Activity} feed=${feed} />`}
      </main>

      <${Drawer} target=${target} report=${report} act=${act}
                 onClose=${() => setTarget(null)} />
      ${notice &&
      html`
        <div class="toast ${notice.state}" role="status">
          <b>${notice.label}</b> · ${notice.message}
        </div>
      `}
      ${palette && html`<${Palette} report=${report} go=${go} onClose=${() => setPalette(false)} />`}
    </div>
  `;
}

render(h(App), document.getElementById("root"));
