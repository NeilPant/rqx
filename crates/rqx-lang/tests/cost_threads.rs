//! The cost extraction gives the same answer for a file whichever thread reads it and
//! in whatever order the files are read: nothing in it depends on shared state.

use std::path::{Path, PathBuf};

use rqx_core::Language;
use rqx_core::config::RqxConfig;
use rqx_core::cost::FileCost;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn source_files(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            source_files(&path, into);
        } else if Language::of_path(&path).is_some() {
            into.push(path);
        }
    }
}

fn extraction(path: &Path, config: &RqxConfig) -> FileCost {
    let source = std::fs::read_to_string(path).expect("a fixture reads");
    rqx_lang::analyze(path, &path.to_string_lossy(), &source, config)
        .map(|analysis| analysis.cost)
        .unwrap_or_default()
}

#[test]
fn sixteen_threads_extract_what_one_thread_does() {
    let mut files = Vec::new();
    source_files(&fixtures(), &mut files);
    files.sort();
    assert!(files.len() > 100, "{} fixtures", files.len());
    let config = RqxConfig::default();

    let alone: Vec<FileCost> = files.iter().map(|path| extraction(path, &config)).collect();
    assert!(
        alone.iter().filter(|cost| !cost.is_empty()).count() > 20,
        "the fixtures should hold functions worth recording"
    );

    let threads = 16;
    let mut together: Vec<(usize, FileCost)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|offset| {
                let (files, config) = (&files, &config);
                scope.spawn(move || {
                    files
                        .iter()
                        .enumerate()
                        .skip(offset)
                        .step_by(threads)
                        .map(|(index, path)| (index, extraction(path, config)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a worker finishes"))
            .collect()
    });
    together.sort_by_key(|(index, _)| *index);

    for ((path, one), (_, many)) in files.iter().zip(&alone).zip(&together) {
        assert_eq!(
            one,
            many,
            "{} differs between one thread and sixteen",
            path.display()
        );
    }
}
