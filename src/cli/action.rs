use crate::plan::{Engine, RunRequest};

pub(crate) enum Rendered {
    Stdout(String),
    Exit(i32),
}

#[derive(Clone, Copy)]
pub(crate) enum PlanMode {
    Run,
    DryRun,
    ExplainHuman,
    ExplainJson,
}

pub(crate) enum Action {
    Plan {
        request: Box<RunRequest>,
        mode: PlanMode,
    },
    Doctor {
        json: bool,
    },
    Cleanup {
        dry_run: bool,
        json: bool,
    },
    ImageBuild {
        dry_run: bool,
        engine: Engine,
        tag: String,
        host_mise_tools: bool,
        host_pi_version: bool,
        pi_version: Option<String>,
    },
    Help,
}
