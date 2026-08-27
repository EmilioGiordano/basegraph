//! Source rendering for a synthetic repo: the anchored module at each stage,
//! the duplicate-drift wrapper, procedurally generated filler modules, lib.rs
//! and Cargo.toml.

use crate::scenarios::{Scenario, Variant};

/// The identifiers a repo state is rendered with; drift changes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    pub fn_name: String,
    pub arg: String,
    pub module: String,
    pub crate_name: String,
}

impl Params {
    pub fn fill(&self, template: &str) -> String {
        template
            .replace("@FN@", &self.fn_name)
            .replace("@ARG@", &self.arg)
            .replace("@MOD@", &self.module)
            .replace("@CRATE@", &self.crate_name)
    }

    pub fn anchor_file(&self) -> String {
        format!("src/{}.rs", self.module)
    }
}

/// Which version of the anchored function a module carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    C1,
    C2,
    C3Variant,
}

impl Stage {
    pub fn anchor_fn(self, s: &Scenario) -> &'static str {
        match self {
            Stage::C1 => s.fn_c1,
            Stage::C2 => s.fn_c2,
            Stage::C3Variant => s.fn_c2_variant,
        }
    }
}

pub fn anchor_module(s: &Scenario, stage: Stage, p: &Params) -> String {
    let regression = match stage {
        Stage::C1 => None,
        Stage::C2 | Stage::C3Variant => Some(s.test_regression),
    };
    compose(s, s.types, stage.anchor_fn(s), "", regression, p)
}

/// The module after a reference fix, applied on top of `base_fn` (the C3 fn).
pub fn fixed_module(s: &Scenario, base_fn: &str, v: &Variant, p: &Params) -> String {
    compose(
        s,
        v.types.unwrap_or(s.types),
        v.anchor_fn.unwrap_or(base_fn),
        v.extras,
        Some(s.test_regression),
        p,
    )
}

fn compose(
    s: &Scenario,
    types: &str,
    fn_src: &str,
    extras: &str,
    regression: Option<&str>,
    p: &Params,
) -> String {
    let mut out = format!("//! {}\n\n", s.module_doc);
    out.push_str(types);
    out.push('\n');
    out.push_str(fn_src);
    if !extras.is_empty() {
        out.push('\n');
        out.push_str(extras);
    }
    out.push_str("\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
    out.push_str(s.tests_base);
    if let Some(r) = regression {
        out.push('\n');
        out.push_str(r);
    }
    out.push_str("}\n");
    p.fill(&out)
}

pub fn legacy_module_name(module: &str) -> String {
    format!("legacy_{module}")
}

/// Duplicate drift: a same-named forwarding wrapper in a second module.
pub fn legacy_module(s: &Scenario, p: &Params) -> String {
    let text = format!(
        "//! Compatibility shim: the @MOD@ API as kept for callers that have not migrated.\n\n\
         use crate::@MOD@::*;\n\n\
         /// Forwarding wrapper; the implementation lives in `@MOD@`.\n\
         pub {} {{\n    crate::@MOD@::{}\n}}\n",
        s.sig, s.forward
    );
    p.fill(&text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillerRole {
    Plain,
    /// Imports and exercises the anchored function.
    Caller,
    /// Like `Caller`, but through the duplicate-drift wrapper.
    LegacyCaller,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillerSpec {
    pub name: String,
    pub noun: String,
    pub verb: String,
    pub role: FillerRole,
    pub has_trait: bool,
    pub has_enum: bool,
    pub has_max: bool,
    pub reserve: u64,
    /// A previous module whose `_reserve` this one adds to its budget.
    pub prev: Option<String>,
    /// Helper functions added by noise / body-change commits.
    pub extra_fns: usize,
}

impl FillerSpec {
    pub fn file(&self) -> String {
        format!("src/{}.rs", self.name)
    }
}

pub fn camel(snake: &str) -> String {
    snake
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

pub fn filler_module(f: &FillerSpec, s: &Scenario, p: &Params) -> String {
    let noun = &f.noun;
    let name = &f.name;
    let record = format!("{noun}Record");
    let mut out = format!("//! {noun} bookkeeping.\n\n");

    let types = if s.types_import.is_empty() {
        String::new()
    } else {
        format!(", {}", s.types_import)
    };
    match f.role {
        FillerRole::Plain => {}
        FillerRole::Caller => out.push_str(&format!("use crate::@MOD@::{{@FN@{types}}};\n")),
        FillerRole::LegacyCaller => {
            if !s.types_import.is_empty() {
                out.push_str(&format!("use crate::@MOD@::{{{}}};\n", s.types_import));
            }
            out.push_str("use crate::legacy_@MOD@::@FN@;\n");
        }
    }
    if let Some(prev) = &f.prev {
        out.push_str(&format!("use crate::{prev}::{prev}_reserve;\n"));
    }
    if f.role != FillerRole::Plain || f.prev.is_some() {
        out.push('\n');
    }

    out.push_str(&format!(
        "#[derive(Debug, Clone, PartialEq, Eq)]\n\
         pub struct {record} {{\n    pub id: u64,\n    pub label: String,\n    pub weight: u32,\n}}\n\n\
         impl {record} {{\n    pub fn new(id: u64, label: &str, weight: u32) -> Self {{\n        Self {{\n            id,\n            label: label.to_string(),\n            weight,\n        }}\n    }}\n}}\n\n"
    ));

    if f.has_trait {
        out.push_str(&format!(
            "pub trait {noun}Policy {{\n    fn accept(&self, record: &{record}) -> bool;\n}}\n\n\
             pub struct Weight{noun}Policy {{\n    pub threshold: u32,\n}}\n\n\
             impl {noun}Policy for Weight{noun}Policy {{\n    fn accept(&self, record: &{record}) -> bool {{\n        record.weight >= self.threshold\n    }}\n}}\n\n\
             pub fn {verb}_{name}(records: &[{record}], threshold: u32) -> Vec<{record}> {{\n    let policy = Weight{noun}Policy {{ threshold }};\n    records.iter().filter(|r| policy.accept(r)).cloned().collect()\n}}\n\n",
            verb = f.verb
        ));
    } else {
        out.push_str(&format!(
            "pub fn {verb}_{name}(records: &[{record}], threshold: u32) -> Vec<{record}> {{\n    records.iter().filter(|r| r.weight >= threshold).cloned().collect()\n}}\n\n",
            verb = f.verb
        ));
    }

    if f.has_enum {
        out.push_str(&format!(
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
             pub enum {noun}State {{\n    Pending,\n    Active,\n    Retired,\n}}\n\n\
             pub fn {name}_state(record: &{record}) -> {noun}State {{\n    match record.weight {{\n        0 => {noun}State::Retired,\n        1..=3 => {noun}State::Pending,\n        _ => {noun}State::Active,\n    }}\n}}\n\n"
        ));
    }

    out.push_str(&format!(
        "pub fn {name}_total(records: &[{record}]) -> u64 {{\n    records.iter().map(|r| u64::from(r.weight)).sum()\n}}\n\n\
         pub fn {name}_reserve() -> u64 {{\n    {}\n}}\n",
        f.reserve
    ));

    if f.has_max {
        out.push_str(&format!(
            "\npub fn {name}_heaviest(records: &[{record}]) -> Option<&{record}> {{\n    records.iter().max_by_key(|r| r.weight)\n}}\n"
        ));
    }
    if let Some(prev) = &f.prev {
        out.push_str(&format!(
            "\npub fn {name}_budget(records: &[{record}]) -> u64 {{\n    {name}_total(records) + {name}_reserve() + {prev}_reserve()\n}}\n"
        ));
    }
    if f.role != FillerRole::Plain {
        out.push_str(&format!(
            "\n/// Capacity hint derived from the current schedule.\npub fn {name}_capacity_hint() -> usize {{\n    {}\n}}\n",
            s.filler_call
        ));
    }
    for i in 1..=f.extra_fns {
        out.push_str(&format!(
            "\npub fn {name}_find_{i}<'a>(records: &'a [{record}], label: &str) -> Option<&'a {record}> {{\n    records.iter().filter(|r| r.weight >= {i}).find(|r| r.label == label)\n}}\n"
        ));
    }

    out.push_str(&format!(
        "\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn {name}_totals_and_filters() {{\n        let records = vec![{record}::new(1, \"a\", 1), {record}::new(2, \"b\", 9)];\n        assert_eq!({name}_total(&records), 10);\n        assert_eq!({verb}_{name}(&records, 5).len(), 1);\n        assert!({name}_reserve() > 0);\n    }}\n}}\n",
        verb = f.verb
    ));
    p.fill(&out)
}

pub fn lib_rs(crate_name: &str, modules: &[String]) -> String {
    let mut out = format!("//! {crate_name}: service library.\n\n");
    let mut sorted: Vec<&String> = modules.iter().collect();
    sorted.sort();
    for m in sorted {
        out.push_str(&format!("pub mod {m};\n"));
    }
    out
}

pub fn cargo_toml(crate_name: &str) -> String {
    format!(
        "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n\n[workspace]\n"
    )
}

pub const GITIGNORE: &str = "/target\nCargo.lock\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios;

    fn params() -> Params {
        Params {
            fn_name: "merge_windows".into(),
            arg: "windows".into(),
            module: "scheduling".into(),
            crate_name: "fleetops".into(),
        }
    }

    #[test]
    fn fill_replaces_every_placeholder() {
        let p = params();
        assert_eq!(
            p.fill("use @CRATE@::@MOD@::@FN@; fn f(@ARG@: u8) {}"),
            "use fleetops::scheduling::merge_windows; fn f(windows: u8) {}"
        );
        assert_eq!(p.anchor_file(), "src/scheduling.rs");
    }

    #[test]
    fn camel_case_joins_parts() {
        assert_eq!(camel("retry_queue"), "RetryQueue");
        assert_eq!(camel("audit"), "Audit");
        assert_eq!(camel(""), "");
    }

    #[test]
    fn anchor_module_has_no_placeholders_and_a_regression_test_from_c2() {
        let s = scenarios::by_id("sorted_output").expect("scenario");
        let p = params();
        let c1 = anchor_module(&s, Stage::C1, &p);
        let c2 = anchor_module(&s, Stage::C2, &p);
        for text in [&c1, &c2] {
            assert!(!text.contains('@'), "{text}");
            assert!(text.contains("pub fn merge_windows(windows: &[Window])"));
            assert!(text.contains("#[cfg(test)]"));
        }
        assert!(!c1.contains("unordered_requests_still_merge"));
        assert!(c2.contains("unordered_requests_still_merge"));
        assert!(syn::parse_file(&c2).is_ok(), "{c2}");
    }

    #[test]
    fn every_scenario_stage_and_fix_parses_as_rust() {
        for s in scenarios::all() {
            let p = Params {
                fn_name: s.fn_renamed.into(),
                arg: s.arg_renamed.into(),
                module: s.module_moved.into(),
                crate_name: "demo".into(),
            };
            for stage in [Stage::C1, Stage::C2, Stage::C3Variant] {
                let text = anchor_module(&s, stage, &p);
                assert!(syn::parse_file(&text).is_ok(), "{}: {text}", s.id);
            }
            let legacy = legacy_module(&s, &p);
            assert!(syn::parse_file(&legacy).is_ok(), "{}: {legacy}", s.id);
            for task in &s.tasks {
                for v in [&task.correct, &task.wrong] {
                    let text = fixed_module(&s, s.fn_c2, v, &p);
                    assert!(!text.contains('@'), "{}: {text}", s.id);
                    assert!(syn::parse_file(&text).is_ok(), "{}: {text}", s.id);
                }
                for t in [task.primary_test, task.oracle_test] {
                    let text = p.fill(t);
                    assert!(syn::parse_file(&text).is_ok(), "{}: {text}", s.id);
                }
            }
        }
    }

    #[test]
    fn filler_modules_parse_in_every_role() {
        let s = scenarios::by_id("non_empty").expect("scenario");
        let p = Params {
            fn_name: s.fn_name.into(),
            arg: s.arg.into(),
            module: s.module.into(),
            crate_name: "demo".into(),
        };
        for role in [
            FillerRole::Plain,
            FillerRole::Caller,
            FillerRole::LegacyCaller,
        ] {
            let f = FillerSpec {
                name: "retry_queue".into(),
                noun: "RetryQueue".into(),
                verb: "select".into(),
                role,
                has_trait: true,
                has_enum: true,
                has_max: true,
                reserve: 64,
                prev: Some("audit".into()),
                extra_fns: 2,
            };
            let text = filler_module(&f, &s, &p);
            assert!(!text.contains('@'), "{text}");
            assert!(syn::parse_file(&text).is_ok(), "{text}");
            assert_eq!(text.contains("candidate_hosts"), role != FillerRole::Plain);
            assert!(text.contains("retry_queue_find_2"));
        }
    }

    #[test]
    fn lib_rs_lists_modules_sorted() {
        let text = lib_rs("demo", &["zeta".into(), "alpha".into()]);
        assert_eq!(
            text,
            "//! demo: service library.\n\npub mod alpha;\npub mod zeta;\n"
        );
    }
}
