# Benchmarks

Times `rqx scan` with hyperfine: cold, warm, and warm after one edit. Needs `hyperfine` and `python3`.

    tools/bench/synth.sh 20                             # 20 copies of fixtures/, no network
    tools/bench/corpus.sh                               # the real projects in corpus.lock
    tools/bench/run.sh target/release/rqx               # time one binary on the synthetic corpus
    tools/bench/run.sh --corpus real --baseline OLD target/release/rqx

Raw numbers land in `target/bench/<scenario>.json`. To bump a real project, change its line in `corpus.lock` in its own commit.
