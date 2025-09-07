use std::sync::Arc;
use jobs::InMemJobs;
use sched_core::{Solver, SolveEnvelope, SolveResult};
use solver_milp::MilpSolver;
use solver_heur::HeurSolver;
use async_trait::async_trait;

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<InMemJobs<DispatchSolver>>,
}

#[derive(Clone)]
pub struct DispatchSolver {
    milp: Arc<MilpSolver>,
    heur: Arc<HeurSolver>,
}

impl DispatchSolver {
    pub fn new() -> Self {
        Self { milp: Arc::new(MilpSolver::new()), heur: Arc::new(HeurSolver::new()) }
    }
}

fn day_of(ts: &str) -> &str { ts.split('.').next().unwrap_or(ts) }

fn mask_matches(m: &types::LockMask, a: &types::Assignment, inst: &types::Instance) -> bool {
    let mut ok = true;
    if !m.courses.is_empty()  { ok &= m.courses.iter().any(|x| x == &a.courseId); }
    if !m.groups.is_empty()   {
        if let Some(c) = inst.courses.iter().find(|c| c.id == a.courseId) {
            ok &= m.groups.iter().any(|g| g == &c.groupId);
        } else { ok = false; }
    }
    if !m.teachers.is_empty() { ok &= m.teachers.iter().any(|t| t == &a.teacherId); }
    if !m.rooms.is_empty()    { ok &= m.rooms.iter().any(|r| r == &a.roomId); }
    if !m.days.is_empty()     {
        let d = match day_of(&a.timeslot.0) {
            "mon" => types::DayOfWeek::Mon,
            "tue" => types::DayOfWeek::Tue,
            "wed" => types::DayOfWeek::Wed,
            "thu" => types::DayOfWeek::Thu,
            "fri" => types::DayOfWeek::Fri,
            "sat" => types::DayOfWeek::Sat,
            _ => types::DayOfWeek::Sun,
        };
        ok &= m.days.iter().any(|x| *x == d);
    }
    if !m.times.is_empty()    { ok &= m.times.iter().any(|t| t == &a.timeslot); }
    ok
}

fn partial_pin_matches_mask(
    m: &types::LockMask,
    p: &types::PartialPin,
    inst: &types::Instance,
) -> bool {
    if !m.times.is_empty() && p.timeslot.is_none() { return false; }
    if !m.rooms.is_empty() && p.roomId.is_none()  { return false; }

    let Some(course) = inst.courses.iter().find(|c| c.id == p.courseId) else {
        return false;
    };

    let fake = types::Assignment {
        courseId: p.courseId.clone(),
        timeslot: p.timeslot.clone().unwrap_or_else(|| types::TimeslotId(String::new())),
        roomId:   p.roomId.clone().unwrap_or_else(|| types::RoomId(String::new())),
        teacherId: course.teacherId.clone(),
    };

    mask_matches(m, &fake, inst)
}

fn apply_masks(mut env: types::SolveEnvelope) -> types::SolveEnvelope {
    if env.masks.is_empty() { return env; }

    use std::collections::HashSet;

    for m in env.masks.iter().filter(|m| m.negate) {
        env.pinned.retain(|a| !mask_matches(m, a, &env.instance));

        env.partial_pins.retain(|p| !partial_pin_matches_mask(m, p, &env.instance));
    }

    let mut pins_set: HashSet<(String,String,String,String)> = env.pinned.iter()
        .map(|a| (a.courseId.0.clone(), a.timeslot.0.clone(), a.roomId.0.clone(), a.teacherId.0.clone()))
        .collect();

    let mut partial: Vec<types::PartialPin> = env.partial_pins.clone();

    for m in &env.masks {
        if m.negate { continue; }
        for a in &env.base {
            if !mask_matches(m, a, &env.instance) { continue; }
            match m.lock {
                types::LockMode::Full => {
                    let key = (a.courseId.0.clone(), a.timeslot.0.clone(), a.roomId.0.clone(), a.teacherId.0.clone());
                    if !pins_set.contains(&key) {
                        env.pinned.push(a.clone());
                        pins_set.insert(key);
                    }
                }
                types::LockMode::TimeslotOnly => {
                    partial.push(types::PartialPin {
                        courseId: a.courseId.clone(),
                        timeslot: Some(a.timeslot.clone()),
                        roomId: None,
                    });
                }
                types::LockMode::RoomOnly => {
                    partial.push(types::PartialPin {
                        courseId: a.courseId.clone(),
                        timeslot: None,
                        roomId: Some(a.roomId.clone()),
                    });
                }
                types::LockMode::TimeAndRoom => {
                    partial.push(types::PartialPin {
                        courseId: a.courseId.clone(),
                        timeslot: Some(a.timeslot.clone()),
                        roomId: Some(a.roomId.clone()),
                    });
                }
            }
        }
    }

    partial.sort_by(|a,b| (&a.courseId.0, &a.timeslot.as_ref().map(|x| &x.0), &a.roomId.as_ref().map(|x| &x.0))
        .cmp(&(&b.courseId.0, &b.timeslot.as_ref().map(|x| &x.0), &b.roomId.as_ref().map(|x| &x.0))));
    partial.dedup_by(|a,b| a.courseId==b.courseId && a.timeslot==b.timeslot && a.roomId==b.roomId);

    env.partial_pins = partial;
    env
}

#[async_trait]
impl Solver for DispatchSolver {
    async fn solve(&self, env: SolveEnvelope) -> anyhow::Result<SolveResult> {
        let env = apply_masks(env);
        match env.params.solver {
            types::SolverKind::Milp => {
                let milp_env = env.clone();
                let mut res = self.milp.solve(env).await?;

                if res.status == "solved" && milp_env.params.repairLocalSearch {
                    let before = res.objective;

                    let steps = milp_env.params.repairSteps
                        .map(|x| x as usize)
                        .unwrap_or_else(|| (res.assignments.len().saturating_mul(5)).max(200));

                    let (imp_assign, imp_obj) =
                        self.heur.improve_from(&milp_env.instance, res.assignments.clone(), &milp_env.pinned, &milp_env.partial_pins, milp_env.params.seed, steps);

                    res.stats["method"] = serde_json::json!("milp+ga");
                    res.stats["improved"] = serde_json::json!(false);
                    res.stats["repair_steps"] = serde_json::json!(steps);

                    if imp_obj < before {
                        res.stats["before_objective"] = serde_json::json!(before);
                        res.stats["after_objective"]  = serde_json::json!(imp_obj);
                        res.stats["improved"] = serde_json::json!(true);

                        res.assignments = imp_assign;
                        res.objective = imp_obj;
                    }
                }
                Ok(res)
            }
            types::SolverKind::Heuristic => {
                self.heur.solve(env).await
            }
        }
    }
}

impl AppState {
    pub fn new_default() -> Self {
        let jobs = InMemJobs::new(DispatchSolver::new());
        Self { jobs: Arc::new(jobs) }
    }
}
