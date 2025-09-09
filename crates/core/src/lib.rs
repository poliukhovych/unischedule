pub mod scoring;

use async_trait::async_trait;
use thiserror::Error;

pub use types::{
    Assignment, Course, Group, Instance, Room, SolveEnvelope, SolveParams, SolveResult, Teacher,
    TimeslotId,
};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("invalid instance: {0}")]
    Msg(String),
}

pub fn validate(inst: &Instance) -> Result<(), ValidationError> {
    let mut errors: Vec<String> = Vec::new();

    if inst.timeslots.is_empty() {
        errors.push("timeslots is empty".into());
    }
    for t in &inst.timeslots {
        if !t.is_valid_format() {
            errors.push(format!("timeslot has invalid format: {}", t.0));
        }
    }

    fn chk_unique<I: ToString>(name: &str, ids: impl Iterator<Item = I>, errors: &mut Vec<String>) {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for id in ids {
            let s = id.to_string();
            if !seen.insert(s.clone()) {
                errors.push(format!("duplicate {name} id: {s}"));
            }
        }
    }
    chk_unique(
        "teacher",
        inst.teachers.iter().map(|x| &x.id.0),
        &mut errors,
    );
    chk_unique("group", inst.groups.iter().map(|x| &x.id.0), &mut errors);
    chk_unique("room", inst.rooms.iter().map(|x| &x.id.0), &mut errors);
    chk_unique("course", inst.courses.iter().map(|x| &x.id.0), &mut errors);

    use std::collections::HashSet;
    let teachers: HashSet<_> = inst.teachers.iter().map(|t| &t.id.0).collect();
    let groups: HashSet<_> = inst.groups.iter().map(|g| &g.id.0).collect();
    let rooms = &inst.rooms;
    let times: HashSet<_> = inst.timeslots.iter().map(|t| &t.0).collect();

    for t in &inst.teachers {
        for slot in &t.available {
            if !times.contains(&slot.0) {
                errors.push(format!(
                    "teacher {} has unavailable slot {}",
                    t.id.0, slot.0
                ));
            }
        }
    }

    for c in &inst.courses {
        if !teachers.contains(&c.teacherId.0) {
            errors.push(format!(
                "course {} references missing teacher {}",
                c.id.0, c.teacherId.0
            ));
        }
        if !groups.contains(&c.groupId.0) {
            errors.push(format!(
                "course {} references missing group {}",
                c.id.0, c.groupId.0
            ));
        }
        if c.countPerWeek == 0 {
            errors.push(format!("course {} has countPerWeek=0", c.id.0));
        }
        if !(c.duration == 1 || c.duration == 2) {
            errors.push(format!(
                "course {} has invalid duration {}",
                c.id.0, c.duration
            ));
        }
        let mut any_room_ok = false;
        'rooms: for r in rooms {
            if r.capacity
                < inst
                    .groups
                    .iter()
                    .find(|g| g.id == c.groupId)
                    .map(|g| g.size)
                    .unwrap_or(0)
            {
                continue;
            }
            for need in &c.needs {
                if !r.equip.contains(need) {
                    continue 'rooms;
                }
            }
            any_room_ok = true;
            break;
        }
        if !any_room_ok {
            errors.push(format!(
                "course {} is unschedulable: no suitable room",
                c.id.0
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::Msg(errors.join("; ")))
    }
}

#[async_trait]
pub trait Solver: Send + Sync + 'static {
    async fn solve(&self, env: SolveEnvelope) -> anyhow::Result<SolveResult>;
}
