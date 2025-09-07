#[cfg(feature = "with-milp")]
mod milp_core;

#[cfg(feature = "with-milp")]
use milp_core::*;

use async_trait::async_trait;
use sched_core::{Solver, SolveEnvelope, SolveResult};
use tracing::info;

use std::collections::{HashMap, HashSet};
use types::{
    Assignment, Course, Group, Instance, Room, Teacher, TimeslotId,
    CourseId, RoomId, TeacherId,
};
use good_lp::Solution;

pub struct MilpSolver;
impl MilpSolver { pub fn new() -> Self { Self } }

#[async_trait]
impl Solver for MilpSolver {
    async fn solve(&self, env: SolveEnvelope) -> anyhow::Result<SolveResult> {
        info!("received instance with {} courses", env.instance.courses.len());
        #[cfg(feature = "with-milp")]
        {
            if let Ok(r) = solve_with_milp(&env).await {
                return Ok(r);
            }
        }
        Ok(solve_greedy(&env.instance))
    }
}

fn solve_greedy(inst: &Instance) -> SolveResult {
    let times: Vec<String> = inst.timeslots.iter().map(|t| t.0.clone()).collect();

    let group_size: HashMap<&str, u32> = inst.groups.iter().map(|g| (g.id.0.as_str(), g.size)).collect();
    let teacher_by_id: HashMap<&str, &Teacher> = inst.teachers.iter().map(|t| (t.id.0.as_str(), t)).collect();

    let mut occ_room: HashMap<(&str, usize), bool> = HashMap::new();
    let mut occ_teacher: HashMap<(&str, usize), bool> = HashMap::new();
    let mut occ_group: HashMap<(&str, usize), bool> = HashMap::new();

    let mut assignments: Vec<Assignment> = Vec::new();
    let mut infeasible = false;

    let is_teacher_available = |teacher: &Teacher, t: usize, dur2: bool| -> bool {
        if teacher.available.is_empty() {
            return !dur2 || (t + 1 < times.len());
        }
        let has_t = teacher.available.iter().any(|x| x.0 == times[t]);
        if !dur2 { return has_t; }
        let has_t1 = t + 1 < times.len() && teacher.available.iter().any(|x| x.0 == times[t+1]);
        has_t && has_t1
    };

    let room_ok_for_course = |room: &Room, course: &Course| -> bool {
        let gsz = group_size.get(course.groupId.0.as_str()).copied().unwrap_or(0);
        if room.capacity < gsz { return false; }
        for need in &course.needs {
            if !room.equip.contains(need) { return false; }
        }
        true
    };

    'course_loop: for c in &inst.courses {
        let dur2 = c.duration == 2;
        let mut placed = 0u32;

        for t in 0..times.len() {
            if dur2 && t + 1 >= times.len() { break; }
            let teacher = match teacher_by_id.get(c.teacherId.0.as_str()) {
                Some(tch) => *tch,
                None => { infeasible = true; break 'course_loop; }
            };
            if !is_teacher_available(teacher, t, dur2) { continue; }

            for r in &inst.rooms {
                if !room_ok_for_course(r, c) { continue; }

                let clash = || -> bool {
                    // room
                    if *occ_room.get(&(r.id.0.as_str(), t)).unwrap_or(&false) { return true; }
                    if dur2 && *occ_room.get(&(r.id.0.as_str(), t+1)).unwrap_or(&false) { return true; }
                    // teacher
                    let tid = teacher.id.0.as_str();
                    if *occ_teacher.get(&(tid, t)).unwrap_or(&false) { return true; }
                    if dur2 && *occ_teacher.get(&(tid, t+1)).unwrap_or(&false) { return true; }
                    // group
                    let gid = c.groupId.0.as_str();
                    if *occ_group.get(&(gid, t)).unwrap_or(&false) { return true; }
                    if dur2 && *occ_group.get(&(gid, t+1)).unwrap_or(&false) { return true; }
                    false
                }();

                if clash { continue; }

                assignments.push(Assignment {
                    courseId: c.id.clone(),
                    timeslot: TimeslotId(times[t].clone()),
                    roomId: r.id.clone(),
                    teacherId: c.teacherId.clone(),
                });

                *occ_room.entry((r.id.0.as_str(), t)).or_default() = true;
                *occ_teacher.entry((teacher.id.0.as_str(), t)).or_default() = true;
                *occ_group.entry((c.groupId.0.as_str(), t)).or_default() = true;
                if dur2 {
                    *occ_room.entry((r.id.0.as_str(), t+1)).or_default() = true;
                    *occ_teacher.entry((teacher.id.0.as_str(), t+1)).or_default() = true;
                    *occ_group.entry((c.groupId.0.as_str(), t+1)).or_default() = true;
                }

                placed += 1;
                if placed == c.countPerWeek { break; }
            }
            if placed == c.countPerWeek {}
        }

        if placed < c.countPerWeek {
            infeasible = true;
        }
    }

    SolveResult {
        status: if infeasible { "infeasible".into() } else { "solved".into() },
        objective: 0.0,
        assignments,
        violations: vec![],
        stats: serde_json::json!({
            "method": "greedy",
            "timeslots": inst.timeslots.len(),
            "courses": inst.courses.len(),
            "rooms": inst.rooms.len()
        }),
    }
}

#[cfg(feature = "with-milp")]
async fn solve_with_milp(env: &types::SolveEnvelope) -> anyhow::Result<SolveResult> {
    use good_lp::{default_solver, ProblemVariables, SolverModel};

    let prep = build_prep(env);

    let mut pvars = ProblemVariables::new();
    let starts = declare_starts(&prep, &mut pvars);
    if starts.is_empty() {
        return Ok(SolveResult {
            status: "infeasible".into(),
            objective: 0.0,
            assignments: env.pinned.clone(),
            violations: vec![],
            stats: serde_json::json!({"method":"milp","note":"no feasible start variables","pinned":env.pinned.len(),"base":env.base.len()}),
        });
    }
    let (ot, og) = declare_occupancy_vars(&prep, &mut pvars);
    let (adj_t, adj_g) = declare_adjacency_vars(&prep, &mut pvars, &ot, &og);
    let v = milp_core::Vars { starts, ot, og, adj_t, adj_g };

    let objective = build_objective(&prep, &v);

    let mut model = pvars.minimise(objective.clone()).using(default_solver);
    model = add_course_count_constraints(model, &prep, &v);
    model = add_room_capacity_constraints(model, &prep, &v);
    model = add_teacher_capacity_constraints(model, &prep, &v);
    model = add_group_capacity_constraints(model, &prep, &v);
    model = link_occupancy(model, &prep, &v);
    model = add_adjacency_constraints(model, &v);
    model = add_partial_lock_constraints(model, &prep, &v);

    match model.solve() {
        Ok(sol) => {
            let assignments = extract_solution(&prep, &v, &sol);
            Ok(SolveResult {
                status: "solved".into(),
                objective: sol.eval(objective.clone()),
                assignments,
                violations: vec![],
                stats: serde_json::json!({
                    "method": "milp",
                    "vars": "starts+ot/og+adj",
                    "timeslots": prep.inst.timeslots.len(),
                    "courses": prep.inst.courses.len(),
                    "rooms": prep.inst.rooms.len(),
                    "pinned": env.pinned.len(),
                    "base": env.base.len()
                }),
            })
        }
        Err(e) => Ok(SolveResult {
            status: "infeasible".into(),
            objective: 0.0,
            assignments: env.pinned.clone(),
            violations: vec![],
            stats: serde_json::json!({"method":"milp","error": e.to_string(),"pinned":env.pinned.len(),"base":env.base.len()}),
        }),
    }
}
