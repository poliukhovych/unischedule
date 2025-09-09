use async_trait::async_trait;
use rand::{seq::SliceRandom, Rng};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sched_core::{SolveEnvelope, SolveResult, Solver};
use std::collections::{HashMap, HashSet};
use types::{Assignment, Course, Instance, Room, Teacher};

pub struct HeurSolver;
impl HeurSolver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Solver for HeurSolver {
    async fn solve(&self, env: SolveEnvelope) -> anyhow::Result<SolveResult> {
        let mut rng = ChaCha8Rng::seed_from_u64(env.params.seed);
        let inst = env.instance;

        let feas = build_feasible(&inst);
        let pinset: HashSet<(String, String, String, String)> =
            env.pinned.iter().map(pin_key).collect();

        let time_locked: HashSet<(String, String)> = env
            .partial_pins
            .iter()
            .filter_map(|p| {
                p.timeslot
                    .as_ref()
                    .map(|t| (p.courseId.0.clone(), t.0.clone()))
            })
            .collect();
        let room_locked: HashSet<(String, String)> = env
            .partial_pins
            .iter()
            .filter_map(|p| {
                p.roomId
                    .as_ref()
                    .map(|r| (p.courseId.0.clone(), r.0.clone()))
            })
            .collect();
        let time_room_locked: HashSet<(String, String, String)> = env
            .partial_pins
            .iter()
            .filter_map(|p| match (&p.timeslot, &p.roomId) {
                (Some(t), Some(r)) => Some((p.courseId.0.clone(), t.0.clone(), r.0.clone())),
                _ => None,
            })
            .collect();

        let pop_size = 40usize.min(10 + inst.courses.len() * 2);
        let iters = 300usize;
        let mut population: Vec<Candidate> = Vec::new();

        if let Some(c0) = randomized_construct_with_pins_and_base(
            &inst,
            &feas,
            &env.pinned,
            &env.base,
            &env.partial_pins,
            &mut rng,
        ) {
            population.push(c0);
        }

        while population.len() < pop_size {
            if let Some(c) = randomized_construct_with_pins_and_base(
                &inst,
                &feas,
                &env.pinned,
                &Vec::new(),
                &env.partial_pins,
                &mut rng,
            ) {
                population.push(c);
            } else {
                break;
            }
        }

        if population.is_empty() {
            return Ok(SolveResult {
                status: "infeasible".into(),
                objective: 0.0,
                assignments: vec![],
                violations: vec![],
                stats: serde_json::json!({"method":"ga","note":"failed to construct with pins"}),
            });
        }
        population.sort_by(|a, b| a.objective.total_cmp(&b.objective));

        for _ in 0..iters {
            let parent = tournament(&population, 3, &mut rng).clone();
            let mut child = mutate(
                &inst,
                &feas,
                parent,
                &mut rng,
                &pinset,
                &time_locked,
                &room_locked,
                &time_room_locked,
            );
            child.evaluate(&inst);
            if let Some(worst) = population.last() {
                if child.objective < worst.objective {
                    population.pop();
                    insert_sorted(&mut population, child);
                }
            } else {
                insert_sorted(&mut population, child);
            }
        }

        let best = &population[0];
        Ok(SolveResult {
            status: "solved".into(),
            objective: best.objective,
            assignments: best.assignments.clone(),
            violations: vec![],
            stats: serde_json::json!({
                "method": "ga",
                "pop": population.len(),
                "best": best.objective,
            }),
        })
    }
}

impl HeurSolver {
    pub fn improve_from(
        &self,
        inst: &types::Instance,
        base: Vec<types::Assignment>,
        pins: &Vec<types::Assignment>,
        locks: &Vec<types::PartialPin>,
        seed: u64,
        steps: usize,
    ) -> (Vec<types::Assignment>, f64) {
        let feas = build_feasible(inst);
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x9E37_79B9_7F4A_7C15);
        let pinset: HashSet<(String, String, String, String)> = pins.iter().map(pin_key).collect();

        let time_locked: HashSet<(String, String)> = locks
            .iter()
            .filter_map(|p| {
                p.timeslot
                    .as_ref()
                    .map(|t| (p.courseId.0.clone(), t.0.clone()))
            })
            .collect();
        let room_locked: HashSet<(String, String)> = locks
            .iter()
            .filter_map(|p| {
                p.roomId
                    .as_ref()
                    .map(|r| (p.courseId.0.clone(), r.0.clone()))
            })
            .collect();
        let time_room_locked: HashSet<(String, String, String)> = locks
            .iter()
            .filter_map(|p| match (&p.timeslot, &p.roomId) {
                (Some(t), Some(r)) => Some((p.courseId.0.clone(), t.0.clone(), r.0.clone())),
                _ => None,
            })
            .collect();

        let mut parent =
            randomized_construct_with_pins_and_base(inst, &feas, pins, &base, locks, &mut rng)
                .unwrap_or_else(|| Candidate {
                    assignments: base,
                    objective: 0.0,
                });
        parent.evaluate(inst);

        for _ in 0..steps {
            let mut child = mutate(
                inst,
                &feas,
                parent.clone(),
                &mut rng,
                &pinset,
                &time_locked,
                &room_locked,
                &time_room_locked,
            );
            child.evaluate(inst);
            if child.objective < parent.objective {
                parent = child;
            }
        }
        (parent.assignments, parent.objective)
    }
}

#[derive(Clone)]
struct Candidate {
    assignments: Vec<Assignment>,
    objective: f64,
}

impl Candidate {
    fn evaluate(&mut self, inst: &Instance) {
        let s = sched_core::scoring::compute_soft_scores(inst, &self.assignments);
        self.objective = s.objective;
    }
}

fn insert_sorted(pop: &mut Vec<Candidate>, c: Candidate) {
    let pos = pop.partition_point(|x| x.objective <= c.objective);
    pop.insert(pos, c);
}

fn build_feasible(inst: &Instance) -> Vec<Vec<(usize, usize)>> {
    let times: Vec<&str> = inst.timeslots.iter().map(|t| t.0.as_str()).collect();
    let group_size: HashMap<&str, u32> = inst
        .groups
        .iter()
        .map(|g| (g.id.0.as_str(), g.size))
        .collect();
    let teacher_by_id: HashMap<&str, &Teacher> =
        inst.teachers.iter().map(|t| (t.id.0.as_str(), t)).collect();

    let room_ok_for_course = |room: &Room, course: &Course| -> bool {
        let gsz = *group_size.get(course.groupId.0.as_str()).unwrap_or(&0);
        if room.capacity < gsz {
            return false;
        }
        for need in &course.needs {
            if !room.equip.contains(need) {
                return false;
            }
        }
        true
    };
    let is_teacher_available = |teacher: &Teacher, t: usize, dur2: bool| -> bool {
        if teacher.available.is_empty() {
            return !dur2 || (t + 1 < times.len());
        }
        let has_t = teacher.available.iter().any(|x| x.0 == times[t]);
        if !dur2 {
            return has_t;
        }
        let has_t1 = t + 1 < times.len() && teacher.available.iter().any(|x| x.0 == times[t + 1]);
        has_t && has_t1
    };

    let mut feas: Vec<Vec<(usize, usize)>> = vec![Vec::new(); inst.courses.len()];
    for (ci, c) in inst.courses.iter().enumerate() {
        let dur2 = c.duration == 2;
        let teacher = match teacher_by_id.get(c.teacherId.0.as_str()) {
            Some(t) => *t,
            None => continue,
        };
        for t in 0..times.len() {
            if dur2 && t + 1 >= times.len() {
                break;
            }
            if !is_teacher_available(teacher, t, dur2) {
                continue;
            }
            for (ri, r) in inst.rooms.iter().enumerate() {
                if room_ok_for_course(r, c) {
                    feas[ci].push((t, ri));
                }
            }
        }
    }
    feas
}

fn pin_key(a: &types::Assignment) -> (String, String, String, String) {
    (
        a.courseId.0.clone(),
        a.timeslot.0.clone(),
        a.roomId.0.clone(),
        a.teacherId.0.clone(),
    )
}

#[derive(Default, Clone)]
struct Occupancy {
    room: HashSet<(usize, usize)>,
    teacher: HashSet<(usize, usize)>,
    group: HashSet<(usize, usize)>,
}

fn randomized_construct(
    inst: &Instance,
    feas: &Vec<Vec<(usize, usize)>>,
    rng: &mut ChaCha8Rng,
) -> Option<Candidate> {
    let times = &inst.timeslots;
    let teacher_index: HashMap<&str, usize> = inst
        .teachers
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.0.as_str(), i))
        .collect();
    let group_index: HashMap<&str, usize> = inst
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id.0.as_str(), i))
        .collect();

    let mut occ = Occupancy::default();
    let mut assignments = Vec::new();

    let mut order: Vec<usize> = (0..inst.courses.len()).collect();
    order.sort_by_key(|&ci| feas[ci].len());

    for &ci in &order {
        let c = &inst.courses[ci];
        let mut placed = 0u32;
        if feas[ci].is_empty() {
            return None;
        }

        let mut starts = feas[ci].clone();
        starts.shuffle(rng);

        'outer: for _attempt in 0..(starts.len().max(50)) {
            let mut local_occ = occ.clone();
            let mut local_ass: Vec<Assignment> = Vec::new();
            let mut used: HashSet<(usize, usize)> = HashSet::new();

            starts.shuffle(rng);

            for &(t, r) in starts.iter() {
                if used.contains(&(t, r)) {
                    continue;
                }
                if !place_ok(ci, c, t, r, &mut local_occ, &teacher_index, &group_index) {
                    continue;
                }
                local_ass.push(Assignment {
                    courseId: c.id.clone(),
                    timeslot: times[t].clone(),
                    roomId: inst.rooms[r].id.clone(),
                    teacherId: c.teacherId.clone(),
                });
                used.insert((t, r));
                placed += 1;
                if placed == c.countPerWeek {
                    occ = local_occ;
                    assignments.extend(local_ass);
                    break 'outer;
                }
            }
            placed = 0;
        }

        if placed < c.countPerWeek {
            return None;
        }
    }

    let mut cand = Candidate {
        assignments,
        objective: 0.0,
    };
    cand.evaluate(inst);
    Some(cand)
}

fn randomized_construct_with_pins_and_base(
    inst: &Instance,
    feas: &Vec<Vec<(usize, usize)>>,
    pins: &Vec<Assignment>,
    base: &Vec<Assignment>,
    locks: &Vec<types::PartialPin>,
    rng: &mut ChaCha8Rng,
) -> Option<Candidate> {
    use std::collections::{HashMap, HashSet};

    let idx_ts: HashMap<&str, usize> = inst
        .timeslots
        .iter()
        .enumerate()
        .map(|(i, t)| (t.0.as_str(), i))
        .collect();
    let idx_room: HashMap<&str, usize> = inst
        .rooms
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.0.as_str(), i))
        .collect();
    let idx_course: HashMap<&str, usize> = inst
        .courses
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.0.as_str(), i))
        .collect();

    let teacher_index: HashMap<&str, usize> = inst
        .teachers
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.0.as_str(), i))
        .collect();
    let group_index: HashMap<&str, usize> = inst
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id.0.as_str(), i))
        .collect();

    let mut occ = Occupancy::default();
    let mut assignments: Vec<Assignment> = Vec::new();
    let mut pinned_set: HashSet<(String, String, String, String)> = HashSet::new();

    for a in pins {
        let (Some(&ci), Some(&ti), Some(&ri)) = (
            idx_course.get(a.courseId.0.as_str()),
            idx_ts.get(a.timeslot.0.as_str()),
            idx_room.get(a.roomId.0.as_str()),
        ) else {
            continue;
        };
        let c = &inst.courses[ci];
        if !place_ok(ci, c, ti, ri, &mut occ, &teacher_index, &group_index) {
            return None;
        }
        assignments.push(a.clone());
        pinned_set.insert(pin_key(a));
    }

    for a in base {
        if pinned_set.contains(&pin_key(a)) {
            continue;
        }
        let (Some(&ci), Some(&ti), Some(&ri)) = (
            idx_course.get(a.courseId.0.as_str()),
            idx_ts.get(a.timeslot.0.as_str()),
            idx_room.get(a.roomId.0.as_str()),
        ) else {
            continue;
        };
        let c = &inst.courses[ci];
        let already = assignments.iter().filter(|x| x.courseId == c.id).count() as u32;
        if already >= c.countPerWeek {
            continue;
        }
        if place_ok(ci, c, ti, ri, &mut occ, &teacher_index, &group_index) {
            assignments.push(a.clone());
        }
    }

    let mut locks_by_course: HashMap<usize, Vec<(Option<usize>, Option<usize>)>> = HashMap::new();
    'locks: for l in locks {
        let Some(&ci) = idx_course.get(l.courseId.0.as_str()) else {
            return None;
        };
        let t_opt = match &l.timeslot {
            Some(ts) => {
                let Some(&ti) = idx_ts.get(ts.0.as_str()) else {
                    return None;
                };
                Some(ti)
            }
            None => None,
        };
        let r_opt = match &l.roomId {
            Some(rr) => {
                let Some(&ri) = idx_room.get(rr.0.as_str()) else {
                    return None;
                };
                Some(ri)
            }
            None => None,
        };

        for a in &assignments {
            if a.courseId != inst.courses[ci].id {
                continue;
            }
            let time_ok = match t_opt {
                Some(ti) => inst.timeslots[ti].0 == a.timeslot.0,
                None => true,
            };
            let room_ok = match r_opt {
                Some(ri) => inst.rooms[ri].id == a.roomId,
                None => true,
            };
            if time_ok && room_ok {
                continue 'locks;
            }
        }

        locks_by_course.entry(ci).or_default().push((t_opt, r_opt));
    }

    let mut order: Vec<usize> = (0..inst.courses.len()).collect();
    order.sort_by_key(|&ci| feas[ci].len());

    for &ci in &order {
        let c = &inst.courses[ci];

        let have0 = assignments.iter().filter(|x| x.courseId == c.id).count() as u32;

        let mut course_locks = locks_by_course.remove(&ci).unwrap_or_default();

        if have0 + (course_locks.len() as u32) > c.countPerWeek {
            return None;
        }

        for (t_req, r_req) in course_locks.drain(..) {
            let mut starts: Vec<(usize, usize)> = feas[ci].clone();
            if let Some(ti) = t_req {
                starts.retain(|(t, _r)| *t == ti);
            }
            if let Some(ri) = r_req {
                starts.retain(|(_t, r)| *r == ri);
            }
            starts.shuffle(rng);

            let mut placed = false;
            for (t, r) in starts {
                if place_ok(ci, c, t, r, &mut occ, &teacher_index, &group_index) {
                    assignments.push(Assignment {
                        courseId: c.id.clone(),
                        timeslot: inst.timeslots[t].clone(),
                        roomId: inst.rooms[r].id.clone(),
                        teacherId: c.teacherId.clone(),
                    });
                    placed = true;
                    break;
                }
            }
            if !placed {
                return None;
            }
        }

        let have = assignments.iter().filter(|x| x.courseId == c.id).count() as u32;
        let need = c.countPerWeek.saturating_sub(have);
        if need == 0 {
            continue;
        }

        let mut starts = feas[ci].clone();
        starts.shuffle(rng);

        let mut placed = 0u32;
        for &(t, r) in &starts {
            if place_ok(ci, c, t, r, &mut occ, &teacher_index, &group_index) {
                assignments.push(Assignment {
                    courseId: c.id.clone(),
                    timeslot: inst.timeslots[t].clone(),
                    roomId: inst.rooms[r].id.clone(),
                    teacherId: c.teacherId.clone(),
                });
                placed += 1;
                if placed == need {
                    break;
                }
            }
        }
        if placed < need {
            return None;
        }
    }

    let mut cand = Candidate {
        assignments,
        objective: 0.0,
    };
    cand.evaluate(inst);
    Some(cand)
}

fn place_ok(
    ci: usize,
    course: &Course,
    t: usize,
    r: usize,
    occ: &mut Occupancy,
    teacher_index: &HashMap<&str, usize>,
    group_index: &HashMap<&str, usize>,
) -> bool {
    let tidx = match teacher_index.get(course.teacherId.0.as_str()) {
        Some(&i) => i,
        None => return false,
    };
    let gidx = match group_index.get(course.groupId.0.as_str()) {
        Some(&i) => i,
        None => return false,
    };
    let dur2 = course.duration == 2;

    if occ.room.contains(&(r, t))
        || occ.teacher.contains(&(tidx, t))
        || occ.group.contains(&(gidx, t))
    {
        return false;
    }
    if dur2 {
        if occ.room.contains(&(r, t + 1))
            || occ.teacher.contains(&(tidx, t + 1))
            || occ.group.contains(&(gidx, t + 1))
        {
            return false;
        }
    }
    occ.room.insert((r, t));
    occ.teacher.insert((tidx, t));
    occ.group.insert((gidx, t));
    if dur2 {
        occ.room.insert((r, t + 1));
        occ.teacher.insert((tidx, t + 1));
        occ.group.insert((gidx, t + 1));
    }
    true
}

fn tournament<'a>(pop: &'a Vec<Candidate>, k: usize, rng: &'a mut ChaCha8Rng) -> &'a Candidate {
    let mut best: Option<&Candidate> = None;
    for _ in 0..k {
        let i = rng.gen_range(0..pop.len());
        let c = &pop[i];
        if best.map_or(true, |b| c.objective < b.objective) {
            best = Some(c);
        }
    }
    best.unwrap()
}

fn mutate(
    inst: &Instance,
    feas: &Vec<Vec<(usize, usize)>>,
    mut parent: Candidate,
    rng: &mut ChaCha8Rng,
    pinned_set: &HashSet<(String, String, String, String)>,
    time_locked: &HashSet<(String, String)>,
    room_locked: &HashSet<(String, String)>,
    time_room_locked: &HashSet<(String, String, String)>,
) -> Candidate {
    if parent.assignments.is_empty() {
        return parent;
    }
    if parent.assignments.len() == pinned_set.len() {
        return parent;
    }

    let teacher_index: HashMap<&str, usize> = inst
        .teachers
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.0.as_str(), i))
        .collect();
    let group_index: HashMap<&str, usize> = inst
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id.0.as_str(), i))
        .collect();

    let mut occ = Occupancy::default();
    let times = &inst.timeslots;

    let mut slots_by_course: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut course_by_id: HashMap<&str, &Course> = HashMap::new();
    for c in &inst.courses {
        course_by_id.insert(c.id.0.as_str(), c);
    }

    for (ai, a) in parent.assignments.iter().enumerate() {
        let c = course_by_id.get(a.courseId.0.as_str()).unwrap();
        let ci = inst.courses.iter().position(|x| x.id == c.id).unwrap();
        let t0 = inst
            .timeslots
            .iter()
            .position(|x| x.0 == a.timeslot.0)
            .unwrap();
        let r = inst.rooms.iter().position(|x| x.id == a.roomId).unwrap();

        let tidx = *teacher_index.get(c.teacherId.0.as_str()).unwrap();
        let gidx = *group_index.get(c.groupId.0.as_str()).unwrap();
        occ.room.insert((r, t0));
        occ.teacher.insert((tidx, t0));
        occ.group.insert((gidx, t0));
        if c.duration == 2 {
            occ.room.insert((r, t0 + 1));
            occ.teacher.insert((tidx, t0 + 1));
            occ.group.insert((gidx, t0 + 1));
        }
        slots_by_course.entry(c.id.0.as_str()).or_default().push(ai);
    }

    let mutations = 1 + (parent.assignments.len() / 10).min(3);
    for _ in 0..mutations {
        let mut tries = 0usize;
        let ai = loop {
            if tries > 10 * parent.assignments.len() {
                return parent;
            }
            let i = rng.gen_range(0..parent.assignments.len());
            let key = (
                parent.assignments[i].courseId.0.clone(),
                parent.assignments[i].timeslot.0.clone(),
                parent.assignments[i].roomId.0.clone(),
                parent.assignments[i].teacherId.0.clone(),
            );
            if !pinned_set.contains(&key) {
                break i;
            }
            tries += 1;
        };

        let a = parent.assignments[ai].clone();
        let c = course_by_id.get(a.courseId.0.as_str()).unwrap();
        let ci = inst.courses.iter().position(|x| x.id == c.id).unwrap();

        let t0 = inst
            .timeslots
            .iter()
            .position(|x| x.0 == a.timeslot.0)
            .unwrap();
        let r0 = inst.rooms.iter().position(|x| x.id == a.roomId).unwrap();
        let tidx = *teacher_index.get(c.teacherId.0.as_str()).unwrap();
        let gidx = *group_index.get(c.groupId.0.as_str()).unwrap();

        if time_room_locked.contains(&(c.id.0.clone(), a.timeslot.0.clone(), a.roomId.0.clone())) {
            continue;
        }

        occ.room.remove(&(r0, t0));
        occ.teacher.remove(&(tidx, t0));
        occ.group.remove(&(gidx, t0));
        if c.duration == 2 {
            occ.room.remove(&(r0, t0 + 1));
            occ.teacher.remove(&(tidx, t0 + 1));
            occ.group.remove(&(gidx, t0 + 1));
        }

        let mut candidates = feas[ci].clone();
        candidates.shuffle(rng);

        if time_locked.contains(&(c.id.0.clone(), a.timeslot.0.clone())) {
            candidates.retain(|(t, _)| *t == t0);
        }

        if room_locked.contains(&(c.id.0.clone(), a.roomId.0.clone())) {
            candidates.retain(|(_, r)| *r == r0);
        }

        let mut placed = false;
        for &(t, r) in &candidates {
            if place_ok(ci, c, t, r, &mut occ, &teacher_index, &group_index) {
                parent.assignments[ai] = Assignment {
                    courseId: c.id.clone(),
                    timeslot: times[t].clone(),
                    roomId: inst.rooms[r].id.clone(),
                    teacherId: c.teacherId.clone(),
                };
                placed = true;
                break;
            }
        }

        if !placed {
            let _ = place_ok(ci, c, t0, r0, &mut occ, &teacher_index, &group_index);
        }
    }

    parent
}
