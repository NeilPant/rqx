//! The composition over cost facts written by hand, so each case is one shape of call
//! graph and nothing else.

use std::path::{Path, PathBuf};

use rqx_core::cost::{
    ArgSize, Bound, CallSite, CalleeRef, Conf, Cost, FileCost, FunctionCost, Indicators, Order,
    Param, Recurrence, Shrink, SizeSrc, SizeVar, Summaries, Summary,
};
use rqx_core::facts::{ExportKind, ExportedSymbol, ImportRequest, ModuleFacts};
use rqx_core::span::Span;

use super::{CallLinks, Reached, Unit, compose_linked};
use crate::cost::tables::CostTables;
use crate::module_graph::ResolvedRequests;

fn order(text: &str) -> Order {
    text.parse().unwrap()
}

/// A function of one file: its parameters, each also a size variable, and no cost yet.
fn function(qual: &str, start: u32, parameters: &[&str]) -> FunctionCost {
    FunctionCost {
        name: Some(qual.rsplit('.').next().unwrap().to_owned()),
        qual: qual.to_owned(),
        start,
        line: start / 10 + 1,
        params: parameters
            .iter()
            .map(|name| Param {
                name: (*name).to_owned(),
                type_head: None,
            })
            .collect(),
        vars: parameters
            .iter()
            .enumerate()
            .map(|(position, name)| SizeVar {
                name: (*name).to_owned(),
                src: SizeSrc::Param(position as u8),
            })
            .collect(),
        ..FunctionCost::default()
    }
}

trait Build {
    fn looping(self, position: u16, body: Cost) -> Self;
    fn calling(&mut self, callee: CalleeRef, arguments: &[ArgSize]) -> Cost;
    fn exported(self) -> Self;
}

impl Build for FunctionCost {
    /// Loops over the parameter at `position`, with `body` inside.
    fn looping(mut self, position: u16, body: Cost) -> Self {
        let line = self.line + 1;
        self.time = Cost::Loop {
            bound: Bound::Size(position),
            body: Box::new(body),
            line,
        };
        self
    }

    /// Calls `callee` with `arguments`, and returns the cost of that call.
    fn calling(&mut self, callee: CalleeRef, arguments: &[ArgSize]) -> Cost {
        self.calls.push(CallSite {
            callee,
            args: arguments.to_vec(),
            line: self.line + 1,
            in_loop: 0,
        });
        Cost::Call((self.calls.len() - 1) as u16)
    }

    fn exported(mut self) -> Self {
        self.exported_as = self.name.clone();
        self
    }
}

fn local(name: &str) -> CalleeRef {
    CalleeRef::Local(name.to_owned())
}

fn method(class: Option<&str>, name: &str) -> CalleeRef {
    CalleeRef::Method {
        recv_type: class.map(str::to_owned),
        name: name.to_owned(),
    }
}

/// `walk(items)`: one pass over its only parameter.
fn walk(name: &str, start: u32) -> FunctionCost {
    function(name, start, &["items"]).looping(0, Cost::One)
}

struct File {
    name: &'static str,
    path: PathBuf,
    facts: ModuleFacts,
    cost: FileCost,
}

fn file(name: &'static str, functions: Vec<FunctionCost>) -> File {
    File {
        name,
        path: PathBuf::from(format!("/p/{name}")),
        facts: ModuleFacts::default(),
        cost: FileCost { functions },
    }
}

impl File {
    fn exporting(mut self, names: &[&str]) -> Self {
        self.facts.exports = names
            .iter()
            .map(|name| ExportedSymbol {
                name: (*name).to_owned(),
                span: Span::new(0, 1),
                kind: ExportKind::Function,
            })
            .collect();
        self
    }

    fn importing(mut self, specifier: &str, names: &[&str], re_export: bool) -> Self {
        self.facts.imports.push(ImportRequest {
            specifier: specifier.to_owned(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
            locals: if re_export {
                Vec::new()
            } else {
                names.iter().map(|name| (*name).to_owned()).collect()
            },
            span: Span::new(0, 1),
            takes_everything: false,
            re_export,
        });
        self
    }
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

/// `importer`'s request number `request` names `target`.
fn resolved(requests: &mut ResolvedRequests, importer: &str, request: usize, target: &str) {
    requests
        .entry((path(importer), request))
        .or_default()
        .push(path(target));
}

fn summarise(files: &[File], requests: &ResolvedRequests) -> Summaries {
    summarise_linked(files, requests).0
}

fn summarise_linked(files: &[File], requests: &ResolvedRequests) -> (Summaries, CallLinks) {
    let units: Vec<Unit<'_>> = files
        .iter()
        .map(|file| Unit {
            name: file.name,
            path: Path::new(&file.path),
            facts: &file.facts,
            cost: &file.cost,
        })
        .collect();
    compose_linked(&units, requests, &CostTables::built_in(), true)
}

fn summary<'s>(all: &'s Summaries, file: &File, qual: &str) -> &'s Summary {
    let start = file
        .cost
        .functions
        .iter()
        .find(|function| function.qual == qual)
        .unwrap_or_else(|| panic!("no function {qual}"))
        .start;
    all.get(file.name, start)
        .unwrap_or_else(|| panic!("no summary for {qual}"))
}

#[test]
fn a_loop_around_a_linear_call_is_quadratic() {
    let mut outer = function("outer", 100, &["items"]);
    let call = outer.calling(local("scan"), &[ArgSize::Param(0)]);
    let outer = outer.looping(0, call);
    let files = [file("a.ts", vec![walk("scan", 10), outer])];

    let all = summarise(&files, &ResolvedRequests::new());

    let outer = summary(&all, &files[0], "outer");
    assert_eq!(outer.time, order("O(n²)"));
    assert_eq!(outer.conf, Conf::Verified);
    let steps: Vec<(&str, &str)> = outer
        .trace
        .iter()
        .map(|step| (step.what.as_str(), step.file.as_str()))
        .collect();
    assert_eq!(
        steps,
        [("loop", "a.ts"), ("scan", "a.ts"), ("loop", "a.ts")],
        "{:?}",
        outer.trace
    );
    assert_eq!(outer.trace[0].order, order("O(n²)"));
    assert_eq!(outer.trace[1].order, order("O(n)"));
}

#[test]
fn a_callee_is_costed_in_the_sizes_the_caller_passes_it() {
    let inner = function("inner", 10, &["small", "large"]).looping(1, Cost::One);
    let mut outer = function("outer", 100, &["xs", "ys"]);
    let call = outer.calling(local("inner"), &[ArgSize::Param(0), ArgSize::Param(1)]);
    let swapped = {
        let mut swapped = function("swapped", 200, &["xs", "ys"]);
        let call = swapped.calling(local("inner"), &[ArgSize::Param(1), ArgSize::Param(0)]);
        swapped.looping(0, call)
    };
    let outer = outer.looping(0, call);
    let files = [file("a.ts", vec![inner, outer, swapped])];

    let all = summarise(&files, &ResolvedRequests::new());

    assert_eq!(summary(&all, &files[0], "outer").time, order("O(n·m)"));
    assert_eq!(summary(&all, &files[0], "swapped").time, order("O(n·n)"));
}

#[test]
fn a_constant_argument_makes_the_call_constant_and_an_unknown_one_a_size_of_its_own() {
    let mut fixed = function("fixed", 100, &["xs"]);
    let call = fixed.calling(local("scan"), &[ArgSize::Const]);
    let fixed = fixed.looping(0, call);
    let mut unsure = function("unsure", 200, &["xs"]);
    let call = unsure.calling(local("scan"), &[ArgSize::Unknown]);
    let unsure = unsure.looping(0, call);
    let files = [file("a.ts", vec![walk("scan", 10), fixed, unsure])];

    let all = summarise(&files, &ResolvedRequests::new());

    assert_eq!(summary(&all, &files[0], "fixed").time, order("O(n)"));
    assert_eq!(summary(&all, &files[0], "unsure").time, order("O(n·m)"));
}

#[test]
fn a_call_that_reaches_nothing_known_costs_nothing_and_costs_no_confidence() {
    let mut caller = function("caller", 10, &["xs"]);
    let outside = caller.calling(CalleeRef::Unresolved, &[ArgSize::Param(0)]);
    let missing = caller.calling(local("nowhere"), &[ArgSize::Param(0)]);
    let unknown_import = caller.calling(
        CalleeRef::Imported {
            local: "lodash".to_owned(),
            member: Some("sortBy".to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    caller.time = Cost::Seq(vec![outside, missing, unknown_import]);
    let files = [file("a.ts", vec![caller])];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, Order::one());
    assert_eq!(caller.conf, Conf::Verified);
}

#[test]
fn a_function_is_costed_after_the_ones_it_calls_through_two_levels() {
    let mut middle = function("middle", 50, &["items"]);
    let call = middle.calling(local("scan"), &[ArgSize::Param(0)]);
    middle.time = call;
    let mut top = function("top", 100, &["items"]);
    let call = top.calling(local("middle"), &[ArgSize::Param(0)]);
    let top = top.looping(0, call);
    // Listed top first: nothing depends on the order the functions are written in.
    let files = [file("a.ts", vec![top, middle, walk("scan", 10)])];

    let all = summarise(&files, &ResolvedRequests::new());

    assert_eq!(summary(&all, &files[0], "top").time, order("O(n²)"));
    assert_eq!(summary(&all, &files[0], "middle").time, order("O(n)"));
}

#[test]
fn a_call_to_another_file_follows_the_import() {
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(
        CalleeRef::Imported {
            local: "walk".to_owned(),
            member: Some("walk".to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    let caller = caller.looping(0, call);
    let files = [
        file("b.ts", vec![caller]).importing("./a", &["walk"], false),
        file("a.ts", vec![walk("walk", 10).exported()]).exporting(&["walk"]),
    ];
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "b.ts", 0, "a.ts");

    let all = summarise(&files, &requests);

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, order("O(n²)"));
    assert!(
        caller.trace.iter().any(|step| step.file == "a.ts"),
        "{:?}",
        caller.trace
    );
}

/// `b.ts` calls `find` through a chain of `hops` barrels, each re-exporting it from the
/// next, and the last one defines it.
fn barrel_chain(hops: usize) -> Summaries {
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(
        CalleeRef::Imported {
            local: "find".to_owned(),
            member: Some("find".to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    let caller = caller.looping(0, call);
    let mut files = vec![file("caller.ts", vec![caller]).importing("./b0", &["find"], false)];
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "caller.ts", 0, "b0.ts");
    for hop in 0..hops {
        let name: &'static str = Box::leak(format!("b{hop}.ts").into_boxed_str());
        let next = if hop + 1 == hops {
            "defined".to_owned()
        } else {
            format!("b{}", hop + 1)
        };
        files.push(file(name, Vec::new()).exporting(&["find"]).importing(
            &format!("./{next}"),
            &["find"],
            true,
        ));
        resolved(&mut requests, name, 0, &format!("{next}.ts"));
    }
    files.push(file("defined.ts", vec![walk("find", 10).exported()]).exporting(&["find"]));
    summarise(&files, &requests)
}

#[test]
fn a_call_follows_re_exports_through_a_barrel() {
    let all = barrel_chain(1);
    assert_eq!(
        all.get("caller.ts", 100).unwrap().time,
        order("O(n²)"),
        "{all:?}"
    );
}

#[test]
fn re_exports_are_followed_three_files_deep_and_no_further() {
    assert_eq!(
        barrel_chain(3).get("caller.ts", 100).unwrap().time,
        order("O(n²)")
    );
    assert_eq!(
        barrel_chain(4).get("caller.ts", 100).unwrap().time,
        order("O(n)"),
        "the fourth hop is beyond the limit, so the call reaches nothing"
    );
}

fn caller_of(member: &str) -> FunctionCost {
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(
        CalleeRef::Imported {
            local: member.to_owned(),
            member: Some(member.to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    caller.looping(0, call)
}

#[test]
fn a_re_export_under_another_name_is_found() {
    let files = [
        file("caller.ts", vec![caller_of("renamed")]).importing("./barrel", &["renamed"], false),
        file("barrel.ts", Vec::new())
            .exporting(&["find", "renamed"])
            .importing("./defined", &["find", "other"], true),
        file(
            "defined.ts",
            vec![walk("other", 10).exported(), function("find", 20, &["a"])],
        )
        .exporting(&["other", "find"]),
    ];
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "caller.ts", 0, "barrel.ts");
    resolved(&mut requests, "barrel.ts", 0, "defined.ts");

    let all = summarise(&files, &requests);

    assert_eq!(all.get("caller.ts", 100).unwrap().time, order("O(n\u{b2})"));
}

#[test]
fn a_default_export_is_the_one_function_a_file_exports() {
    let files = [
        file("caller.ts", vec![caller_of("Thing")]).importing("./barrel", &["Thing"], false),
        file("barrel.ts", Vec::new())
            .exporting(&["Thing"])
            .importing("./thing", &["default"], true),
        file("thing.ts", vec![walk("only", 10).exported()]).exporting(&["only"]),
    ];
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "caller.ts", 0, "barrel.ts");
    resolved(&mut requests, "barrel.ts", 0, "thing.ts");

    let all = summarise(&files, &requests);

    assert_eq!(all.get("caller.ts", 100).unwrap().time, order("O(n\u{b2})"));
}

#[test]
fn a_method_on_a_known_class_goes_to_that_class_only() {
    let cheap = function("Cheap.run", 10, &["items"]);
    let costly = walk("Costly.run", 20);
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(method(Some("Cheap"), "run"), &[ArgSize::Param(0)]);
    caller.time = call;
    let files = [file("a.ts", vec![cheap, costly, caller])];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, Order::one());
    assert_eq!(caller.conf, Conf::Verified);
}

#[test]
fn an_unknown_receiver_takes_the_costliest_of_the_methods_of_that_name_as_a_guess() {
    let quadratic = {
        let mut inner = function("B.run", 20, &["items"]);
        let call = inner.calling(local("walk"), &[ArgSize::Param(0)]);
        inner.looping(0, call)
    };
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(method(None, "run"), &[ArgSize::Param(0)]);
    caller.time = call;
    let files = [file(
        "a.ts",
        vec![
            function("A.run", 10, &["items"]),
            quadratic,
            walk("C.run", 30),
            walk("walk", 40),
            caller,
        ],
    )];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, order("O(n²)"));
    assert_eq!(caller.conf, Conf::Heuristic);
}

#[test]
fn a_name_that_too_many_methods_share_reaches_none_of_them() {
    let mut functions: Vec<FunctionCost> = (0..5)
        .map(|index| walk(&format!("K{index}.run"), 10 * (index + 1)))
        .collect();
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(method(None, "run"), &[ArgSize::Param(0)]);
    caller.time = call;
    functions.push(caller);
    let files = [file("a.ts", functions)];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, Order::one());
    assert_eq!(caller.conf, Conf::Verified);
}

#[test]
fn an_overload_is_picked_by_the_number_of_arguments() {
    let one = function("over", 10, &["items"]);
    let two = function("over", 20, &["a", "b"]).looping(0, Cost::One);
    let mut caller = function("caller", 100, &["xs"]);
    let call = caller.calling(local("over"), &[ArgSize::Param(0)]);
    caller.time = call;
    let files = [file("a.ts", vec![one, two, caller])];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, Order::one());
    assert_eq!(caller.conf, Conf::Verified);
}

fn recursive(
    name: &str,
    start: u32,
    rec: Recurrence,
    calls: u8,
    parameters: &[&str],
) -> FunctionCost {
    let mut function = function(name, start, parameters);
    let costs: Vec<Cost> = (0..calls)
        .map(|_| function.calling(local(name), &[ArgSize::Param(0)]))
        .collect();
    function.time = Cost::Seq(costs);
    function.rec = Some(rec);
    function
}

fn rec(calls: u8, shrink: Shrink) -> Recurrence {
    Recurrence {
        calls,
        shrink,
        ..Recurrence::default()
    }
}

#[test]
fn recursion_is_solved_from_how_the_argument_shrinks() {
    let files = [file(
        "a.ts",
        vec![
            recursive("factorial", 10, rec(1, Shrink::Dec1), 1, &["n"]),
            recursive("fib", 20, rec(2, Shrink::Dec1), 2, &["n"]),
            recursive(
                "fib_memo",
                30,
                Recurrence {
                    memo: true,
                    ..rec(2, Shrink::Dec1)
                },
                2,
                &["n"],
            ),
            recursive("power", 40, rec(1, Shrink::Half), 1, &["n"]),
        ],
    )];

    let all = summarise(&files, &ResolvedRequests::new());

    let factorial = summary(&all, &files[0], "factorial");
    assert_eq!(factorial.time, order("O(n)"));
    assert_eq!(factorial.stack, order("O(n)"));
    assert_eq!(factorial.space, order("O(n)"));
    assert_eq!(factorial.conf, Conf::Verified);
    assert!(summary(&all, &files[0], "fib").time.is_exponential());
    assert_eq!(summary(&all, &files[0], "fib").stack, order("O(n)"));
    assert_eq!(summary(&all, &files[0], "fib_memo").time, order("O(n)"));
    let power = summary(&all, &files[0], "power");
    assert_eq!(power.time, order("O(log n)"));
    assert_eq!(power.stack, order("O(log n)"));
    assert_eq!(power.trace[0].what, "recursion");
}

#[test]
fn recursion_is_measured_in_the_parameter_that_shrinks() {
    let mut power = recursive(
        "power",
        10,
        Recurrence {
            param: 1,
            ..rec(1, Shrink::Half)
        },
        1,
        &["base", "exponent"],
    );
    power.calls[0].args = vec![ArgSize::Param(0), ArgSize::Param(1)];
    let mut caller = function("caller", 100, &["xs", "k"]);
    let call = caller.calling(local("power"), &[ArgSize::Const, ArgSize::Param(1)]);
    caller.time = call;
    let files = [file("a.ts", vec![power, caller])];

    let all = summarise(&files, &ResolvedRequests::new());

    // `power(2, k)` takes log k steps, whatever `base` is.
    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, order("O(log m)"), "{caller:?}");
}

#[test]
fn a_halving_recursion_that_merges_its_halves_takes_the_merge_to_be_linear() {
    let mut sort = function("merge_sort", 10, &["items"]);
    let first = sort.calling(local("merge_sort"), &[ArgSize::Unknown]);
    let second = sort.calling(local("merge_sort"), &[ArgSize::Unknown]);
    let merge = Cost::Loop {
        bound: Bound::Unknown,
        body: Box::new(Cost::One),
        line: 4,
    };
    sort.time = Cost::Seq(vec![first, second, merge.clone()]);
    sort.space = merge;
    sort.conf = Conf::Heuristic;
    sort.rec = Some(Recurrence {
        copies_arg: true,
        ..rec(2, Shrink::Half)
    });
    let files = [file("a.ts", vec![sort])];

    let all = summarise(&files, &ResolvedRequests::new());

    let sort = summary(&all, &files[0], "merge_sort");
    assert_eq!(sort.time, order("O(n log n)"));
    assert_eq!(sort.space, order("O(n)"));
    assert_eq!(sort.conf, Conf::Heuristic);
}

#[test]
fn a_recursion_that_works_each_level_multiplies_what_a_level_costs() {
    let mut sum = function("sum_all", 10, &["items"]);
    let call = sum.calling(local("sum_all"), &[ArgSize::Param(0)]);
    let scan = sum.calling(local("scan"), &[ArgSize::Param(0)]);
    sum.time = Cost::Seq(vec![call, scan]);
    sum.rec = Some(rec(1, Shrink::Dec1));
    let files = [file("a.ts", vec![sum, walk("scan", 100)])];

    let all = summarise(&files, &ResolvedRequests::new());

    assert_eq!(summary(&all, &files[0], "sum_all").time, order("O(n²)"));
}

#[test]
fn functions_that_call_one_another_in_a_circle_cost_all_of_them_for_every_level() {
    let mut even = function("is_even", 10, &["n"]);
    let call = even.calling(local("is_odd"), &[ArgSize::Param(0)]);
    even.time = call;
    let mut odd = function("is_odd", 20, &["n"]);
    let call = odd.calling(local("is_even"), &[ArgSize::Param(0)]);
    let scan = odd.calling(local("scan"), &[ArgSize::Param(0)]);
    odd.time = Cost::Seq(vec![call, scan]);
    let mut caller = function("caller", 200, &["xs"]);
    let call = caller.calling(local("is_even"), &[ArgSize::Param(0)]);
    caller.time = call;
    let files = [file("a.ts", vec![even, odd, walk("scan", 100), caller])];

    let all = summarise(&files, &ResolvedRequests::new());

    for name in ["is_even", "is_odd", "caller"] {
        let found = summary(&all, &files[0], name);
        assert_eq!(found.time, order("O(n²)"), "{name}: {found:?}");
        assert_eq!(found.conf, Conf::Heuristic, "{name}");
    }
    assert_eq!(
        summary(&all, &files[0], "is_even").trace[0].what,
        "mutual recursion"
    );
}

#[test]
fn a_circle_across_files_is_one_too() {
    let mut ping = function("ping", 10, &["n"]);
    let call = ping.calling(
        CalleeRef::Imported {
            local: "pong".to_owned(),
            member: Some("pong".to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    ping.time = call;
    ping = ping.exported();
    let mut pong = function("pong", 10, &["n"]);
    let call = pong.calling(
        CalleeRef::Imported {
            local: "ping".to_owned(),
            member: Some("ping".to_owned()),
            req: 0,
        },
        &[ArgSize::Param(0)],
    );
    pong.time = call;
    pong = pong.exported();
    let files = [
        file("a.ts", vec![ping])
            .exporting(&["ping"])
            .importing("./b", &["pong"], false),
        file("b.ts", vec![pong])
            .exporting(&["pong"])
            .importing("./a", &["ping"], false),
    ];
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "a.ts", 0, "b.ts");
    resolved(&mut requests, "b.ts", 0, "a.ts");

    let all = summarise(&files, &requests);

    assert_eq!(all.get("a.ts", 10).unwrap().time, order("O(n)"));
    assert_eq!(all.get("a.ts", 10).unwrap().conf, Conf::Heuristic);
}

#[test]
fn a_loop_around_a_call_that_leaves_the_process_is_as_long_as_the_loop() {
    let mut load = function("load", 10, &["id"]);
    load.ind = Indicators {
        round_trip: true,
        ..Indicators::default()
    };
    let mut load_all = function("load_all", 100, &["ids"]);
    let call = load_all.calling(local("load"), &[ArgSize::Local(1)]);
    let load_all = load_all.looping(0, call);
    let mut once = function("once", 200, &["id"]);
    let call = once.calling(local("load"), &[ArgSize::Param(0)]);
    once.time = call;
    let pure = {
        let mut pure = function("pure_loop", 300, &["ids"]);
        let call = pure.calling(local("scan"), &[ArgSize::Param(0)]);
        pure.looping(0, call)
    };
    let files = [file(
        "a.ts",
        vec![load, load_all, once, pure, walk("scan", 400)],
    )];

    let all = summarise(&files, &ResolvedRequests::new());

    assert_eq!(summary(&all, &files[0], "load_all").io, order("O(n)"));
    assert_eq!(summary(&all, &files[0], "once").io, Order::one());
    assert_eq!(summary(&all, &files[0], "pure_loop").io, Order::one());
}

#[test]
fn the_dominant_step_of_equal_orders_is_the_one_nearest_the_top() {
    let mut caller = function("caller", 100, &["xs"]);
    let first = caller.calling(local("later"), &[ArgSize::Param(0)]);
    let second = caller.calling(local("earlier"), &[ArgSize::Param(0)]);
    caller.calls[0].line = 30;
    caller.calls[1].line = 20;
    caller.time = Cost::Seq(vec![first, second]);
    let files = [file(
        "a.ts",
        vec![walk("later", 10), walk("earlier", 50), caller],
    )];

    let all = summarise(&files, &ResolvedRequests::new());

    let caller = summary(&all, &files[0], "caller");
    assert_eq!(caller.time, order("O(n)"));
    assert_eq!(caller.trace[0].what, "earlier");
}

#[test]
fn a_trace_is_at_most_six_steps() {
    let mut functions = vec![walk("f0", 10)];
    for level in 1..10u32 {
        let mut next = function(&format!("f{level}"), 10 + level * 10, &["items"]);
        let call = next.calling(local(&format!("f{}", level - 1)), &[ArgSize::Param(0)]);
        functions.push(next.looping(0, call));
    }
    let files = [file("a.ts", functions)];

    let all = summarise(&files, &ResolvedRequests::new());

    let top = summary(&all, &files[0], "f9");
    assert_eq!(top.trace.len(), 6);
    assert_eq!(top.time, order("O(n^10)"));
}

#[test]
fn the_answer_does_not_depend_on_the_order_the_files_arrive_in() {
    let build = || {
        let mut caller = function("caller", 100, &["xs"]);
        let call = caller.calling(
            CalleeRef::Imported {
                local: "walk".to_owned(),
                member: Some("walk".to_owned()),
                req: 0,
            },
            &[ArgSize::Param(0)],
        );
        let caller = caller.looping(0, call);
        vec![
            file("b.ts", vec![caller]).importing("./a", &["walk"], false),
            file("a.ts", vec![walk("walk", 10).exported()]).exporting(&["walk"]),
            file("c.ts", vec![walk("other", 5)]),
        ]
    };
    let mut requests = ResolvedRequests::new();
    resolved(&mut requests, "b.ts", 0, "a.ts");
    let forward = build();
    let mut backward = build();
    backward.reverse();

    assert_eq!(
        summarise(&forward, &requests),
        summarise(&backward, &requests)
    );
}

#[test]
fn each_call_is_linked_to_the_functions_that_cost_something() {
    let mut outer = function("outer", 100, &["items"]);
    let costly = outer.calling(local("scan"), &[ArgSize::Param(0)]);
    let cheap = outer.calling(local("trim"), &[ArgSize::Param(0)]);
    let outer = outer.looping(0, Cost::Seq(vec![costly, cheap]));
    let trim = function("trim", 50, &["items"]);
    let files = [file("a.ts", vec![walk("scan", 10), trim, outer])];

    let (_, links) = summarise_linked(&files, &ResolvedRequests::new());

    let reached = links.of_function("a.ts", 100);
    assert_eq!(reached.len(), 2);
    assert_eq!(
        reached[0],
        [Reached {
            file: "a.ts".to_owned(),
            start: 10,
            guessed: false
        }]
    );
    assert!(reached[1].is_empty(), "a constant callee is not a step");
    assert!(links.of_function("a.ts", 10).is_empty());
}

/// Each level runs three loops over collections nothing names, each calling the next
/// level with an argument nothing is known about. Giving every unknown size of a callee
/// a variable of its own grew the order threefold per level; the composed orders must
/// stay within the cap however deep the chain goes.
#[test]
fn a_deep_chain_of_calls_over_unnamed_sizes_stays_short() {
    const DEPTH: u32 = 30;
    let functions: Vec<FunctionCost> = (0..DEPTH)
        .map(|level| {
            let mut function = function(&format!("level{level}"), 1000 * (level + 1), &["xs"]);
            let loops: Vec<Cost> = (0..3)
                .map(|nth| {
                    let body = if level + 1 < DEPTH {
                        function.calling(local(&format!("level{}", level + 1)), &[ArgSize::Unknown])
                    } else {
                        Cost::One
                    };
                    Cost::Loop {
                        bound: Bound::Unknown,
                        body: Box::new(body),
                        line: function.line + 1 + nth,
                    }
                })
                .collect();
            function.time = Cost::Seq(loops);
            function
        })
        .collect();
    let files = [file("chain.ts", functions)];

    let all = summarise(&files, &ResolvedRequests::new());

    for level in 0..DEPTH {
        let time = &summary(&all, &files[0], &format!("level{level}")).time;
        assert!(
            time.terms().len() <= super::fold::MOST_TERMS,
            "level{level}: {time}"
        );
    }
    assert!(summary(&all, &files[0], "level0").time.rank() > 0);
}
