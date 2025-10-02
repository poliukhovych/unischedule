#![allow(clippy::needless_lifetimes)]

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, Variable};
use std::collections::{HashMap, HashSet};
use types::{Assignment, Course, Instance, Room, Teacher, TimeslotId};

pub(crate) struct PartialLock {
    pub c: usize,
    pub t: Option<usize>,
    pub r: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct StartVar {
    pub c: usize,
    pub t: usize,
    pub r: usize,
    pub var: Variable,
}

pub(crate) struct PinnedState<'a> {
    pub vec: Vec<Assignment>,
    pub room: HashMap<(usize, usize), bool>,
    pub teacher: HashMap<(&'a str, usize), bool>,
    pub group: HashMap<(&'a str, usize), bool>,
    pub count_by_course: HashMap<usize, u32>,
    pub unpref_pinned_count: i64,
}

pub(crate) struct Prep<'a> {
    pub inst: &'a Instance,
    pub times: Vec<&'a str>,
    pub day_slots: HashMap<&'a str, Vec<usize>>,
    pub group_size: HashMap<&'a str, u32>,
    pub teacher_by_id: HashMap<&'a str, &'a Teacher>,
    pub avoid_by_teacher: HashMap<&'a str, HashSet<&'a str>>,
    pub idx_ts: HashMap<&'a str, usize>,
    pub idx_room: HashMap<&'a str, usize>,
    pub idx_course: HashMap<&'a str, usize>,
    pub teacher_ids: Vec<&'a str>,
    pub group_ids: Vec<&'a str>,
    pub pinned: PinnedState<'a>,
    pub locks: Vec<PartialLock>,
}

pub(crate) struct Vars<'a> {
    pub starts: Vec<StartVar>,
    pub ot: HashMap<(&'a str, usize), Variable>,
    pub og: HashMap<(&'a str, usize), Variable>,
    pub adj_t: Vec<(Variable, (&'a str, usize), (&'a str, usize))>,
    pub adj_g: Vec<(Variable, (&'a str, usize), (&'a str, usize))>,
}

mod prep {
    use types::TimeslotId;
    pub fn compute_ts_index(all: &Vec<TimeslotId>, needle: &str) -> Option<usize> {
        all.iter().position(|x| x.0 == needle)
    }
}

pub(crate) fn compute_day_slots<'a>(times: &Vec<&'a str>) -> HashMap<&'a str, Vec<usize>> {
    let mut day_of: Vec<&str> = Vec::with_capacity(times.len());
    let mut day_index: Vec<u32> = Vec::with_capacity(times.len());
    for &ts in times {
        let mut parts = ts.split('.');
        let d = parts.next().unwrap_or("");
        let idx = parts
            .next()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(0);
        day_of.push(d);
        day_index.push(idx);
    }
    let mut day_slots: HashMap<&str, Vec<usize>> = HashMap::new();
    for k in 0..times.len() {
        day_slots.entry(day_of[k]).or_default().push(k);
    }
    for v in day_slots.values_mut() {
        v.sort_by_key(|&k| day_index[k]);
    }
    day_slots
}

pub(crate) fn compute_avoid_by_teacher<'a>(
    inst: &'a Instance,
) -> HashMap<&'a str, HashSet<&'a str>> {
    let mut avoid_by_teacher: HashMap<&str, HashSet<&str>> = HashMap::new();
    for t in &inst.teachers {
        avoid_by_teacher.insert(
            t.id.0.as_str(),
            t.prefs.avoid_slots.iter().map(|s| s.0.as_str()).collect(),
        );
    }
    avoid_by_teacher
}

pub(crate) fn compute_indices<'a>(
    inst: &'a Instance,
) -> (
    HashMap<&'a str, usize>,
    HashMap<&'a str, usize>,
    HashMap<&'a str, usize>,
) {
    let idx_ts = inst
        .timeslots
        .iter()
        .enumerate()
        .map(|(i, t)| (t.0.as_str(), i))
        .collect();
    let idx_room = inst
        .rooms
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.0.as_str(), i))
        .collect();
    let idx_course = inst
        .courses
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.0.as_str(), i))
        .collect();
    (idx_ts, idx_room, idx_course)
}

pub(crate) fn teacher_available(
    teacher: &Teacher,
    times: &Vec<&str>,
    t: usize,
    dur2: bool,
) -> bool {
    if teacher.available.is_empty() {
        return !dur2 || (t + 1 < times.len());
    }
    let has_t = teacher.available.iter().any(|x| x.0 == times[t]);
    if !dur2 {
        return has_t;
    }
    let has_t1 = t + 1 < times.len() && teacher.available.iter().any(|x| x.0 == times[t + 1]);
    has_t && has_t1
}

pub(crate) fn room_ok_for_course(
    room: &Room,
    course: &Course,
    group_size: &HashMap<&str, u32>,
) -> bool {
    let gsz = group_size
        .get(course.groupId.0.as_str())
        .copied()
        .unwrap_or(0);
    if room.capacity < gsz {
        return false;
    }
    for need in &course.needs {
        if !room.equip.contains(need) {
            return false;
        }
    }
    true
}

pub(crate) fn occupies(courses: &Vec<Course>, s: &StartVar, k: usize) -> bool {
    if courses[s.c].duration == 1 {
        s.t == k
    } else {
        s.t == k || s.t + 1 == k
    }
}

pub(crate) fn build_pinned<'a>(
    env: &'a types::SolveEnvelope,
    inst: &'a Instance,
    times: &Vec<&'a str>,
    avoid_by_teacher: &HashMap<&'a str, HashSet<&'a str>>,
) -> PinnedState<'a> {
    let (idx_ts, idx_room, idx_course) = compute_indices(inst);

    let mut room: HashMap<(usize, usize), bool> = HashMap::new();
    let mut teacher: HashMap<(&'a str, usize), bool> = HashMap::new();
    let mut group: HashMap<(&'a str, usize), bool> = HashMap::new();
    let mut count_by_course: HashMap<usize, u32> = HashMap::new();
    let mut unpref_pinned_count: i64 = 0;
    let mut vec: Vec<Assignment> = Vec::new();

    for a in &env.pinned {
        let (Some(&ci), Some(&ti), Some(&ri)) = (
            idx_course.get(a.courseId.0.as_str()),
            idx_ts.get(a.timeslot.0.as_str()),
            idx_room.get(a.roomId.0.as_str()),
        ) else {
            continue;
        };

        vec.push(a.clone());
        *count_by_course.entry(ci).or_default() += 1;

        let c = &inst.courses[ci];
        let dur2 = c.duration == 2;

        room.insert((ri, ti), true);
        if dur2 && ti + 1 < inst.timeslots.len() {
            room.insert((ri, ti + 1), true);
        }

        let tid = a.teacherId.0.as_str();
        teacher.insert((tid, ti), true);
        if dur2 && ti + 1 < inst.timeslots.len() {
            teacher.insert((tid, ti + 1), true);
        }

        let gid = c.groupId.0.as_str();
        group.insert((gid, ti), true);
        if dur2 && ti + 1 < inst.timeslots.len() {
            group.insert((gid, ti + 1), true);
        }

        if let Some(avoid) = avoid_by_teacher.get(&tid) {
            let mut penalize = avoid.contains(times[ti]);
            if dur2 && ti + 1 < times.len() {
                penalize = penalize || avoid.contains(times[ti + 1]);
            }
            if penalize {
                unpref_pinned_count += 1;
            }
        }
    }

    PinnedState {
        vec,
        room,
        teacher,
        group,
        count_by_course,
        unpref_pinned_count,
    }
}

pub(crate) fn build_prep<'a>(env: &'a types::SolveEnvelope) -> Prep<'a> {
    let inst = &env.instance;
    let times: Vec<&str> = inst.timeslots.iter().map(|t| t.0.as_str()).collect();
    let day_slots = compute_day_slots(&times);
    let group_size: HashMap<&str, u32> = inst
        .groups
        .iter()
        .map(|g| (g.id.0.as_str(), g.size))
        .collect();
    let teacher_by_id: HashMap<&str, &Teacher> =
        inst.teachers.iter().map(|t| (t.id.0.as_str(), t)).collect();
    let avoid_by_teacher = compute_avoid_by_teacher(inst);
    let (idx_ts, idx_room, idx_course) = compute_indices(inst);

    let mut teacher_ids: Vec<&str> = {
        let mut t = HashSet::new();
        inst.courses.iter().for_each(|c| {
            t.insert(c.teacherId.0.as_str());
        });
        t.into_iter().collect()
    };
    let group_ids: Vec<&str> = {
        let mut g = HashSet::new();
        inst.courses.iter().for_each(|c| {
            g.insert(c.groupId.0.as_str());
        });
        g.into_iter().collect()
    };

    let pinned = build_pinned(env, inst, &times, &avoid_by_teacher);
    for a in &env.pinned {
        if !teacher_ids.iter().any(|&x| x == a.teacherId.0.as_str()) {
            teacher_ids.push(a.teacherId.0.as_str());
        }
    }

    let mut locks: Vec<PartialLock> = Vec::new();
    for l in &env.partial_pins {
        if let Some(&ci) = idx_course.get(l.courseId.0.as_str()) {
            let t = l
                .timeslot
                .as_ref()
                .and_then(|ts| prep::compute_ts_index(&inst.timeslots, &ts.0));
            let r = l
                .roomId
                .as_ref()
                .and_then(|rr| inst.rooms.iter().position(|x| x.id == *rr))
                .map(|x| x);
            locks.push(PartialLock { c: ci, t, r });
        }
    }

    Prep {
        inst,
        times,
        day_slots,
        group_size,
        teacher_by_id,
        avoid_by_teacher,
        idx_ts,
        idx_room,
        idx_course,
        teacher_ids,
        group_ids,
        pinned,
        locks,
    }
}

pub(crate) fn declare_starts<'a>(prep: &'a Prep, vars: &mut ProblemVariables) -> Vec<StartVar> {
    let mut starts = Vec::new();
    for (ci, c) in prep.inst.courses.iter().enumerate() {
        let dur2 = c.duration == 2;
        let teacher = match prep.teacher_by_id.get(c.teacherId.0.as_str()) {
            Some(t) => *t,
            None => continue,
        };
        for t in 0..prep.times.len() {
            if dur2 && t + 1 >= prep.times.len() {
                break;
            }
            if !teacher_available(teacher, &prep.times, t, dur2) {
                continue;
            }
            for (ri, r) in prep.inst.rooms.iter().enumerate() {
                if !room_ok_for_course(r, c, &prep.group_size) {
                    continue;
                }
                if *prep.pinned.room.get(&(ri, t)).unwrap_or(&false) {
                    continue;
                }
                if dur2 && *prep.pinned.room.get(&(ri, t + 1)).unwrap_or(&false) {
                    continue;
                }
                if *prep
                    .pinned
                    .teacher
                    .get(&(c.teacherId.0.as_str(), t))
                    .unwrap_or(&false)
                {
                    continue;
                }
                if dur2
                    && *prep
                        .pinned
                        .teacher
                        .get(&(c.teacherId.0.as_str(), t + 1))
                        .unwrap_or(&false)
                {
                    continue;
                }
                if *prep
                    .pinned
                    .group
                    .get(&(c.groupId.0.as_str(), t))
                    .unwrap_or(&false)
                {
                    continue;
                }
                if dur2
                    && *prep
                        .pinned
                        .group
                        .get(&(c.groupId.0.as_str(), t + 1))
                        .unwrap_or(&false)
                {
                    continue;
                }

                let v = vars.add(good_lp::variable().binary());
                starts.push(StartVar {
                    c: ci,
                    t,
                    r: ri,
                    var: v,
                });
            }
        }
    }
    starts
}

pub(crate) fn declare_occupancy_vars<'a>(
    prep: &'a Prep,
    vars: &mut ProblemVariables,
) -> (
    HashMap<(&'a str, usize), Variable>,
    HashMap<(&'a str, usize), Variable>,
) {
    let mut ot = HashMap::new();
    for &tid in &prep.teacher_ids {
        for k in 0..prep.times.len() {
            ot.insert((tid, k), vars.add(good_lp::variable().binary()));
        }
    }
    let mut og = HashMap::new();
    for &gid in &prep.group_ids {
        for k in 0..prep.times.len() {
            og.insert((gid, k), vars.add(good_lp::variable().binary()));
        }
    }
    (ot, og)
}

pub(crate) fn declare_adjacency_vars<'a>(
    prep: &'a Prep,
    vars: &mut ProblemVariables,
    _ot: &HashMap<(&'a str, usize), Variable>,
    _og: &HashMap<(&'a str, usize), Variable>,
) -> (
    Vec<(Variable, (&'a str, usize), (&'a str, usize))>,
    Vec<(Variable, (&'a str, usize), (&'a str, usize))>,
) {
    let mut adj_t = Vec::new();
    for &tid in &prep.teacher_ids {
        for (_day, slots) in &prep.day_slots {
            for w in slots.windows(2) {
                let a = vars.add(good_lp::variable().binary());
                adj_t.push((a, (tid, w[0]), (tid, w[1])));
            }
        }
    }
    let mut adj_g = Vec::new();
    for &gid in &prep.group_ids {
        for (_day, slots) in &prep.day_slots {
            for w in slots.windows(2) {
                let a = vars.add(good_lp::variable().binary());
                adj_g.push((a, (gid, w[0]), (gid, w[1])));
            }
        }
    }
    (adj_t, adj_g)
}

pub(crate) fn build_objective(prep: &Prep, v: &Vars) -> Expression {
    let mut objective = Expression::from(0.0);
    let w_unpref = prep.inst.policy.soft_weights.unpreferred_time as f64;
    let w_windows = prep.inst.policy.soft_weights.windows as f64;

    if w_unpref > 0.0 {
        for s in &v.starts {
            let c = &prep.inst.courses[s.c];
            if let Some(avoid) = prep.avoid_by_teacher.get(&c.teacherId.0.as_str()) {
                let mut penalize = avoid.contains(prep.times[s.t]);
                if c.duration == 2 && s.t + 1 < prep.times.len() {
                    penalize = penalize || avoid.contains(prep.times[s.t + 1]);
                }
                if penalize {
                    objective = objective + w_unpref * s.var;
                }
            }
        }
        if prep.pinned.unpref_pinned_count > 0 {
            objective = objective + w_unpref * (prep.pinned.unpref_pinned_count as f64);
        }
    }

    if w_windows > 0.0 {
        for &tid in &prep.teacher_ids {
            for (_day, slots) in &prep.day_slots {
                if slots.len() < 2 {
                    continue;
                }
                for &k in slots {
                    objective = objective + w_windows * v.ot[&(tid, k)];
                }
            }
        }
        for &(a, (tid, _k), (_tid2, _k1)) in &v.adj_t {
            debug_assert_eq!(tid, _tid2);
            objective = objective - w_windows * a;
        }
        for &gid in &prep.group_ids {
            for (_day, slots) in &prep.day_slots {
                if slots.len() < 2 {
                    continue;
                }
                for &k in slots {
                    objective = objective + w_windows * v.og[&(gid, k)];
                }
            }
        }
        for &(a, (gid, _k), (_gid2, _k1)) in &v.adj_g {
            debug_assert_eq!(gid, _gid2);
            objective = objective - w_windows * a;
        }
    }

    objective
}

pub(crate) fn add_course_count_constraints<M: SolverModel>(
    mut model: M,
    prep: &Prep,
    v: &Vars,
) -> M {
    for (ci, c) in prep.inst.courses.iter().enumerate() {
        let mut sum = Expression::from(0.0);
        for s in v.starts.iter().filter(|s| s.c == ci) {
            sum = sum + s.var;
        }
        let pinned_cnt = *prep.pinned.count_by_course.get(&ci).unwrap_or(&0);
        let need = c.countPerWeek.saturating_sub(pinned_cnt);
        model = model.with(sum.eq(need as f64));
    }
    model
}

pub(crate) fn add_room_capacity_constraints<M: SolverModel>(
    mut model: M,
    prep: &Prep,
    v: &Vars,
) -> M {
    for (ri, _r) in prep.inst.rooms.iter().enumerate() {
        for k in 0..prep.times.len() {
            let mut sum = Expression::from(0.0);
            for s in v
                .starts
                .iter()
                .filter(|s| s.r == ri && occupies(&prep.inst.courses, s, k))
            {
                sum = sum + s.var;
            }
            let rhs = if *prep.pinned.room.get(&(ri, k)).unwrap_or(&false) {
                0.0
            } else {
                1.0
            };
            model = model.with(sum.leq(rhs));
        }
    }
    model
}

pub(crate) fn add_teacher_capacity_constraints<M: SolverModel>(
    mut model: M,
    prep: &Prep,
    v: &Vars,
) -> M {
    for &tid in &prep.teacher_ids {
        for k in 0..prep.times.len() {
            let mut sum = Expression::from(0.0);
            for s in v.starts.iter().filter(|s| {
                prep.inst.courses[s.c].teacherId.0.as_str() == tid
                    && occupies(&prep.inst.courses, s, k)
            }) {
                sum = sum + s.var;
            }
            let rhs = if *prep.pinned.teacher.get(&(tid, k)).unwrap_or(&false) {
                0.0
            } else {
                1.0
            };
            model = model.with(sum.leq(rhs));
        }
    }
    model
}

pub(crate) fn add_group_capacity_constraints<M: SolverModel>(
    mut model: M,
    prep: &Prep,
    v: &Vars,
) -> M {
    for &gid in &prep.group_ids {
        for k in 0..prep.times.len() {
            let mut sum = Expression::from(0.0);
            for s in v.starts.iter().filter(|s| {
                prep.inst.courses[s.c].groupId.0.as_str() == gid
                    && occupies(&prep.inst.courses, s, k)
            }) {
                sum = sum + s.var;
            }
            let rhs = if *prep.pinned.group.get(&(gid, k)).unwrap_or(&false) {
                0.0
            } else {
                1.0
            };
            model = model.with(sum.leq(rhs));
        }
    }
    model
}

pub(crate) fn link_occupancy<M: SolverModel>(mut model: M, prep: &Prep, v: &Vars) -> M {
    for (&(tid, k), var) in &v.ot {
        let mut sum = Expression::from(0.0);
        for s in v.starts.iter().filter(|s| {
            prep.inst.courses[s.c].teacherId.0.as_str() == tid && occupies(&prep.inst.courses, s, k)
        }) {
            sum = sum + s.var;
        }
        let pinned = if *prep.pinned.teacher.get(&(tid, k)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        model = model.with((sum + pinned).eq(*var));
    }
    for (&(gid, k), var) in &v.og {
        let mut sum = Expression::from(0.0);
        for s in v.starts.iter().filter(|s| {
            prep.inst.courses[s.c].groupId.0.as_str() == gid && occupies(&prep.inst.courses, s, k)
        }) {
            sum = sum + s.var;
        }
        let pinned = if *prep.pinned.group.get(&(gid, k)).unwrap_or(&false) {
            1.0
        } else {
            0.0
        };
        model = model.with((sum + pinned).eq(*var));
    }
    model
}

pub(crate) fn add_adjacency_constraints<M: SolverModel>(mut model: M, v: &Vars) -> M {
    for &(a, (tid, k), (_tid2, k1)) in &v.adj_t {
        model = model.with((a - v.ot[&(tid, k)]).leq(0.0));
        model = model.with((a - v.ot[&(tid, k1)]).leq(0.0));
        model = model.with((a - v.ot[&(tid, k)] - v.ot[&(tid, k1)]).geq(-1.0));
    }
    for &(a, (gid, k), (_gid2, k1)) in &v.adj_g {
        model = model.with((a - v.og[&(gid, k)]).leq(0.0));
        model = model.with((a - v.og[&(gid, k1)]).leq(0.0));
        model = model.with((a - v.og[&(gid, k)] - v.og[&(gid, k1)]).geq(-1.0));
    }
    model
}

pub(crate) fn extract_solution(prep: &Prep, v: &Vars, sol: &impl Solution) -> Vec<Assignment> {
    let mut assignments: Vec<Assignment> = prep.pinned.vec.clone();
    for s in &v.starts {
        if sol.value(s.var) > 0.5 {
            let c = &prep.inst.courses[s.c];
            let r = &prep.inst.rooms[s.r];
            assignments.push(Assignment {
                courseId: c.id.clone(),
                timeslot: TimeslotId(prep.times[s.t].to_string()),
                roomId: r.id.clone(),
                teacherId: c.teacherId.clone(),
            });
        }
    }
    assignments
}

pub(crate) fn add_partial_lock_constraints<M: SolverModel>(
    mut model: M,
    prep: &Prep,
    v: &Vars,
) -> M {
    for lk in &prep.locks {
        let mut sum = Expression::from(0.0);
        for s in v.starts.iter().filter(|s| {
            s.c == lk.c && lk.t.map_or(true, |ti| s.t == ti) && lk.r.map_or(true, |ri| s.r == ri)
        }) {
            sum = sum + s.var;
        }
        model = model.with(sum.eq(1.0));
    }
    model
}
