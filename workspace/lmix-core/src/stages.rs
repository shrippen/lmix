//! Stages of the show and where they show on this machine (show mode
//! VIS.1): names, sizes and patches are kept and checked here; the
//! pictures on them are the engine's.

use lmix_types::stage::{
    Stage, StagePatch, StageState, STAGES, STAGE_MAX_PX, STAGE_MIN_PX, STAGE_OUTPUTS,
};
use lmix_types::ActionError;

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadStage {
        reason: reason.into(),
    }
}

/// Add `stage` to `stages` or replace the one of its name.
pub fn set_stage(stages: &mut Vec<Stage>, stage: Stage) -> Result<(), ActionError> {
    let name = stage.name.trim();
    if name.is_empty() {
        return Err(bad("a stage needs a name"));
    }
    let side = STAGE_MIN_PX..=STAGE_MAX_PX;
    if !side.contains(&stage.width) || !side.contains(&stage.height) {
        return Err(bad(format!(
            "a stage is {STAGE_MIN_PX}–{STAGE_MAX_PX} px wide and high"
        )));
    }
    if stage.width % 2 == 1 || stage.height % 2 == 1 {
        return Err(bad("a stage has even sides"));
    }
    let stage = Stage {
        name: name.to_string(),
        ..stage
    };
    match stages.iter().position(|s| s.name == stage.name) {
        Some(i) => stages[i] = stage,
        None if stages.len() >= STAGES => return Err(bad(format!("at most {STAGES} stages"))),
        None => stages.push(stage),
    }
    Ok(())
}

/// Add `patch` or replace the one of its stage. The stage need not be in
/// the current show: patches belong to the machine.
pub fn set_patch(patches: &mut Vec<StagePatch>, patch: StagePatch) -> Result<(), ActionError> {
    if patch.stage.trim().is_empty() {
        return Err(bad("a patch names its stage"));
    }
    if patch.outputs.is_empty() || patch.outputs.len() > STAGE_OUTPUTS {
        return Err(bad(format!("1–{STAGE_OUTPUTS} screens per stage")));
    }
    match patches.iter().position(|p| p.stage == patch.stage) {
        Some(i) => patches[i] = patch,
        None if patches.len() >= STAGES * 4 => return Err(bad("too many stage patches")),
        None => patches.push(patch),
    }
    Ok(())
}

/// The show's stages with this machine's screens.
pub fn states(stages: &[Stage], patches: &[StagePatch]) -> Vec<StageState> {
    stages
        .iter()
        .map(|stage| StageState {
            name: stage.name.clone(),
            width: stage.width,
            height: stage.height,
            outputs: patches
                .iter()
                .find(|p| p.stage == stage.name)
                .map(|p| p.outputs.clone())
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::stage::StageOutput;
    use lmix_types::DisplayMode;

    fn stage(name: &str, width: u32, height: u32) -> Stage {
        Stage {
            name: name.into(),
            width,
            height,
        }
    }

    #[test]
    fn stages_and_patches_are_checked() {
        let mut stages = Vec::new();
        set_stage(&mut stages, stage(" Back ", 1920, 1080)).unwrap();
        set_stage(&mut stages, stage("Back", 1280, 720)).unwrap();
        assert_eq!(stages, [stage("Back", 1280, 720)], "same name: replaced");
        assert!(set_stage(&mut stages, stage("", 1920, 1080)).is_err());
        assert!(set_stage(&mut stages, stage("Odd", 1919, 1080)).is_err());
        assert!(set_stage(&mut stages, stage("Huge", 9000, 1080)).is_err());

        let mut patches = Vec::new();
        let screen = StageOutput {
            display: DisplayMode::Fullscreen,
        };
        let patch = |outputs: Vec<StageOutput>| StagePatch {
            stage: "Back".into(),
            outputs,
        };
        assert!(set_patch(&mut patches, patch(vec![])).is_err());
        set_patch(&mut patches, patch(vec![screen])).unwrap();
        let states = states(&stages, &patches);
        assert_eq!(states[0].outputs, [screen]);
        assert!(super::states(&[stage("Side", 640, 480)], &patches)[0]
            .outputs
            .is_empty());
    }
}
