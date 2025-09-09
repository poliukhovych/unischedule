use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use utoipa::ToSchema;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(
            Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Eq, PartialEq, Hash,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
id_newtype!(TeacherId);
id_newtype!(GroupId);
id_newtype!(RoomId);
id_newtype!(CourseId);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Eq, PartialEq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DayOfWeek {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Equip {
    Projector,
    Whiteboard,
    ComputerLab,
    Online,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum CourseKind {
    #[default]
    Lecture,
    Lab,
    Seminar,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct TimeslotId(pub String);

impl TimeslotId {
    pub fn is_valid_format(&self) -> bool {
        let parts: Vec<_> = self.0.split('.').collect();
        if parts.len() != 2 {
            return false;
        }
        let day = parts[0];
        let idx_ok = parts[1].parse::<u32>().is_ok();
        matches!(day, "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun") && idx_ok
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct TeacherPrefs {
    #[serde(default)]
    pub preferred_days: Vec<DayOfWeek>,
    #[serde(default)]
    pub avoid_slots: Vec<TimeslotId>,
    #[serde(default)]
    pub morning: bool,
    #[serde(default)]
    pub max_per_day: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Teacher {
    pub id: TeacherId,
    #[serde(default)]
    pub available: Vec<TimeslotId>,
    #[serde(default)]
    pub prefs: TeacherPrefs,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Group {
    pub id: GroupId,
    pub size: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Room {
    pub id: RoomId,
    pub capacity: u32,
    #[serde(default)]
    pub equip: Vec<Equip>,
    #[serde(default)]
    pub building: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Course {
    pub id: CourseId,
    pub groupId: GroupId,
    pub teacherId: TeacherId,
    pub countPerWeek: u32,
    pub duration: u32,
    #[serde(default)]
    pub kind: CourseKind,
    #[serde(default)]
    pub needs: Vec<Equip>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Default)]
pub struct SoftWeights {
    #[serde(default)]
    pub unpreferred_time: i32,
    #[serde(default)]
    pub windows: i32,
    #[serde(default)]
    pub building_switch: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema, Default)]
pub struct Policy {
    #[serde(default)]
    pub soft_weights: SoftWeights,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Instance {
    pub teachers: Vec<Teacher>,
    pub groups: Vec<Group>,
    pub rooms: Vec<Room>,
    pub courses: Vec<Course>,
    pub timeslots: Vec<TimeslotId>,
    pub policy: Policy,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub enum SolverKind {
    Milp,
    Heuristic,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct SolveParams {
    pub solver: SolverKind,
    pub timeLimitSec: u64,
    pub seed: u64,
    pub repairLocalSearch: bool,
    #[serde(default)]
    pub repairSteps: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Assignment {
    pub courseId: CourseId,
    pub timeslot: TimeslotId,
    pub roomId: RoomId,
    pub teacherId: TeacherId,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Violation {
    pub r#type: String,
    pub weight: i64,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct SolveResult {
    pub status: String,
    pub objective: f64,
    pub assignments: Vec<Assignment>,
    pub violations: Vec<Violation>,
    pub stats: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct SolveEnvelope {
    pub instance: Instance,
    pub params: SolveParams,
    #[serde(default)]
    pub base: Vec<Assignment>,
    #[serde(default)]
    pub pinned: Vec<Assignment>,
    #[serde(default)]
    pub masks: Vec<LockMask>,
    #[serde(default)]
    pub partial_pins: Vec<PartialPin>,
}

impl Instance {
    pub fn timeslot_set(&self) -> HashSet<&TimeslotId> {
        self.timeslots.iter().collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LockMode {
    Full,
    TimeslotOnly,
    RoomOnly,
    TimeAndRoom,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LockMask {
    #[serde(default)]
    pub courses: Vec<CourseId>,
    #[serde(default)]
    pub groups: Vec<GroupId>,
    #[serde(default)]
    pub teachers: Vec<TeacherId>,
    #[serde(default)]
    pub rooms: Vec<RoomId>,
    #[serde(default)]
    pub days: Vec<DayOfWeek>,
    #[serde(default)]
    pub times: Vec<TimeslotId>,
    pub lock: LockMode,
    #[serde(default)]
    pub negate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartialPin {
    pub courseId: CourseId,
    #[serde(default)]
    pub timeslot: Option<TimeslotId>,
    #[serde(default)]
    pub roomId: Option<RoomId>,
}
