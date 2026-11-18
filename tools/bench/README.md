# Benchmarks

Times `rqx scan` with hyperfine in three scenarios: cold, warm, and warm after one edit. Requires `hyperfine` and `python3`. The summary (`summary.py`) reads the JSON of hyperfine 1.x and 2.x, and computes the mean from the individual run times when the export holds none. `python3 -I tools/bench/test_summary.py` checks it against each format.

    tools/bench/synth.sh 20                             # 20 copies of fixtures/, no network
    tools/bench/corpus.sh                               # the real projects in corpus.lock
    tools/bench/run.sh target/release/rqx               # time one binary on the synthetic corpus
    tools/bench/run.sh --corpus real --baseline OLD target/release/rqx

Raw results are written to `target/bench/<scenario>.json`. To update a real project in the corpus, change its line in `corpus.lock` in a separate commit.
