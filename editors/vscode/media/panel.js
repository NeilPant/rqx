// @ts-check
(function () {
  const vscode = acquireVsCodeApi();
  const root = document.getElementById("root");
  const owl = document.body.dataset.owl || "";
  const SEVERITIES = ["error", "warning", "info"];
  const GRADES = ["critical", "poor", "fair", "clean", "pristine"];

  let state = Object.assign({ report: null }, vscode.getState() || {});

  const save = () => vscode.setState(state);

  // Everything below reaches the DOM as a string, and all of it arrives from a
  // subprocess's stdout. The CSP blocks an injected <script>, but not an onerror=
  // smuggled through an attribute.
  const escape = (text) =>
    String(text).replace(
      /[&<>"']/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
    );
  const number = (value) => (Number.isFinite(Number(value)) ? Number(value) : 0);
  const token = (value) => String(value).replace(/[^a-z0-9-]/gi, "");

  /// A verdict in words. The number alone makes the reader do the grading.
  function verdict(health, findings) {
    if (findings === 0) return "Nothing to fix.";
    if (health >= 90) return "In good shape.";
    if (health >= 70) return "Worth a look.";
    if (health >= 40) return "Needs attention.";
    return "Under strain.";
  }

  function distribution(label, counts, keys, prefix) {
    const total = keys.reduce((sum, key) => sum + number(counts[key]), 0);
    if (total === 0) return "";
    const bar = keys
      .filter((key) => number(counts[key]) > 0)
      .map((key) => `<span class="${prefix}${token(key)}" style="flex-grow:${number(counts[key])}"></span>`)
      .join("");
    const legend = keys
      .filter((key) => number(counts[key]) > 0)
      .map(
        (key) =>
          `<span><i class="dot ${prefix}${token(key)}"></i><b>${number(counts[key])}</b> ${escape(key)}</span>`,
      )
      .join("");
    return `<h2>${escape(label)}</h2><div class="bar">${bar}</div><div class="legend">${legend}</div>`;
  }

  function distributions(report) {
    const grades = {};
    for (const grade of Object.values(report.grades || {})) {
      grades[grade] = (grades[grade] || 0) + 1;
    }
    const severity = distribution("Severity", report.counts?.by_severity || {}, SEVERITIES, "");
    const spread = distribution("Grades", grades, GRADES, "grade-");
    if (!severity && !spread) {
      return `<p class="none">Every file came back clean.</p>`;
    }
    return severity + spread;
  }

  function startHere(report) {
    const ranked = report.startHere || [];
    if (ranked.length === 0) {
      return `<p class="none">Nothing stands out. Every file is holding up.</p>`;
    }
    const rows = ranked
      .map((entry) => {
        const bits = [escape(entry.grade)];
        if (number(entry.findings) > 0) bits.push(`${number(entry.findings)} findings`);
        if (number(entry.importers) > 0) bits.push(`${number(entry.importers)} importers`);
        return `<li><button class="row" data-file="${escape(entry.file)}">
          <span class="path">${escape(entry.file)}</span>
          <span class="meta"><i class="dot grade-${token(entry.grade)}"></i>${bits.join(" · ")}</span>
        </button></li>`;
      })
      .join("");
    return `<h2>Start here</h2><ol>${rows}</ol>`;
  }

  function draw() {
    if (!state.report) {
      root.innerHTML = `<div class="empty">
        <img class="owl" src="${owl}" alt="" width="44" height="44" />
        <strong>Nothing scanned yet.</strong>
        <p>Read the project and rqx will grade every file in it.</p>
        <button class="action" id="scan">Scan workspace</button>
      </div>`;
      root.querySelector("#scan")?.addEventListener("click", () => {
        vscode.postMessage({ type: "scan" });
      });
      return;
    }

    const report = state.report;
    const health = number(report.health);
    const findings = number(report.counts?.total);
    const scanned = Object.keys(report.grades || {}).length;

    root.innerHTML = `
      <header class="hero">
        <img class="owl" src="${owl}" alt="" width="44" height="44" />
        <span class="score"><span id="tally">${health}</span><span class="of">/100</span></span>
        <span class="verdict">${escape(verdict(health, findings))}</span>
        <span class="facts">${findings} findings · ${scanned} files · ${number(report.elapsedMs)}ms</span>
      </header>
      <main>
        <section class="pane">${distributions(report)}</section>
        <section class="pane">${startHere(report)}</section>
      </main>`;

    countTo(health);
    wire();
  }

  /// The score counts up rather than appearing. It is the one number the panel exists
  /// to show, and a figure that lands is read; one that is simply there is skipped.
  function countTo(target) {
    const tally = root.querySelector("#tally");
    if (!tally || target === 0) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const started = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - started) / 900);
      // Ease-out cubic: fast to settle, no overshoot. Nothing here should bounce.
      tally.textContent = String(Math.round(target * (1 - Math.pow(1 - t, 3))));
      if (t < 1) requestAnimationFrame(step);
    };
    tally.textContent = "0";
    requestAnimationFrame(step);
  }

  function wire() {
    for (const button of root.querySelectorAll("button.row")) {
      button.addEventListener("click", () =>
        vscode.postMessage({ type: "open", file: button.dataset.file }),
      );
    }
  }

  function render() {
    try {
      draw();
    } catch (error) {
      // A report saved by an earlier build can be missing fields this one reads, and a
      // panel that throws here stays blank with no way back.
      state.report = null;
      save();
      draw();
    }
  }

  window.addEventListener("message", (event) => {
    const message = event.data;
    if (message.type === "report") state.report = message.report;
    save();
    render();
  });

  render();
  vscode.postMessage({ type: "ready" });
})();
