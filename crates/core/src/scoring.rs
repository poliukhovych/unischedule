use std::collections::{HashMap, HashSet};
use types::{Assignment, Course, Instance, Room, Teacher};

#[derive(Clone, Debug, Default)]
pub struct Scores {
    pub unpreferred_meetings: i64,
    pub windows_teachers: HashMap<String, i64>,
    pub windows_groups: HashMap<String, i64>,
    pub windows_total: i64,
    pub objective: f64,
}

pub fn compute_soft_scores(inst: &Instance, assignments: &[Assignment]) -> Scores {
    let times: Vec<&str> = inst.timeslots.iter().map(|t| t.0.as_str()).collect();

    let mut course_by_id: HashMap<&str, &Course> = HashMap::new();
    for c in &inst.courses {
        course_by_id.insert(c.id.0.as_str(), c);
    }

    let teachers_by_id: HashMap<&str, &Teacher> =
        inst.teachers.iter().map(|t| (t.id.0.as_str(), t)).collect();

    let mut day_of: Vec<&str> = Vec::with_capacity(times.len());
    let mut day_index: Vec<u32> = Vec::with_capacity(times.len());
    for &ts in &times {
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

    let mut occ_teacher: HashMap<(&str, usize), bool> = HashMap::new();
    let mut occ_group: HashMap<(&str, usize), bool> = HashMap::new();

    let mut avoid_by_teacher: HashMap<&str, HashSet<&str>> = HashMap::new();
    for t in &inst.teachers {
        avoid_by_teacher.insert(
            t.id.0.as_str(),
            t.prefs.avoid_slots.iter().map(|s| s.0.as_str()).collect(),
        );
    }

    let mut unpref = 0i64;

    let mut ts_index: HashMap<&str, usize> = HashMap::new();
    for (i, &ts) in times.iter().enumerate() {
        ts_index.insert(ts, i);
    }

    for a in assignments {
        let c = match course_by_id.get(a.courseId.0.as_str()) {
            Some(c) => *c,
            None => continue,
        };
        let tid = c.teacherId.0.as_str();
        let gid = c.groupId.0.as_str();
        let t0 = match ts_index.get(a.timeslot.0.as_str()) {
            Some(&i) => i,
            None => continue,
        };
        let dur2 = c.duration == 2;

        if let Some(avoid) = avoid_by_teacher.get(&tid) {
            let mut penalize = avoid.contains(times[t0]);
            if dur2 && t0 + 1 < times.len() {
                penalize = penalize || avoid.contains(times[t0 + 1]);
            }
            if penalize {
                unpref += 1;
            }
        }

        occ_teacher.insert((tid, t0), true);
        occ_group.insert((gid, t0), true);
        if dur2 && t0 + 1 < times.len() {
            occ_teacher.insert((tid, t0 + 1), true);
            occ_group.insert((gid, t0 + 1), true);
        }
    }

    let mut windows_teachers: HashMap<String, i64> = HashMap::new();
    let mut windows_groups: HashMap<String, i64> = HashMap::new();

    let mut teacher_ids: Vec<&str> = inst.teachers.iter().map(|t| t.id.0.as_str()).collect();
    teacher_ids.sort_unstable();
    let mut group_ids: Vec<&str> = inst.groups.iter().map(|g| g.id.0.as_str()).collect();
    group_ids.sort_unstable();

    let mut agent_windows = |is_teacher: bool, id: &str| -> i64 {
        let mut total = 0i64;
        for (_day, slots) in &day_slots {
            if slots.len() < 2 {}
            let mut sum_o = 0i64;
            let mut sum_adj = 0i64;
            for &k in slots {
                let occ = if is_teacher {
                    *occ_teacher.get(&(id, k)).unwrap_or(&false)
                } else {
                    *occ_group.get(&(id, k)).unwrap_or(&false)
                };
                if occ {
                    sum_o += 1;
                }
            }
            for w in slots.windows(2) {
                let k = w[0];
                let k1 = w[1];
                let occ_k = if is_teacher {
                    *occ_teacher.get(&(id, k)).unwrap_or(&false)
                } else {
                    *occ_group.get(&(id, k)).unwrap_or(&false)
                };
                let occ_k1 = if is_teacher {
                    *occ_teacher.get(&(id, k1)).unwrap_or(&false)
                } else {
                    *occ_group.get(&(id, k1)).unwrap_or(&false)
                };
                if occ_k && occ_k1 {
                    sum_adj += 1;
                }
            }
            total += sum_o - sum_adj;
        }
        total
    };

    for &tid in &teacher_ids {
        let val = agent_windows(true, tid);
        if val != 0 {
            windows_teachers.insert(tid.to_string(), val);
        }
    }
    for &gid in &group_ids {
        let val = agent_windows(false, gid);
        if val != 0 {
            windows_groups.insert(gid.to_string(), val);
        }
    }

    let windows_total: i64 =
        windows_teachers.values().sum::<i64>() + windows_groups.values().sum::<i64>();

    let w_unpref = inst.policy.soft_weights.unpreferred_time as f64;
    let w_windows = inst.policy.soft_weights.windows as f64;
    let objective = w_unpref * (unpref as f64) + w_windows * (windows_total as f64);

    Scores {
        unpreferred_meetings: unpref,
        windows_teachers,
        windows_groups,
        windows_total,
        objective,
    }
}
