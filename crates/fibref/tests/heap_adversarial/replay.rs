//! The machinery behind the random-program checks (`acyclic`,
//! `cyclic`, `fault`): a deterministic generator, a plain reference
//! counter written from `spec/ownership.md` §2 that never trusts the
//! heap's counts, a replay of the heap's own trace that checks the
//! trace is well formed as it goes, one random legal step, and the
//! check that heap, model and replay agree on every object.

use std::collections::HashMap;

use fibref::{AuditError, Event, Heap, Kind, ObjId, Value};

/// A deterministic generator (a 64-bit LCG), so a failure reproduces.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0)");
        (self.next() % n as u64) as usize
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len())])
        }
    }
}

/// One object of the plain counting model.
pub struct Shadow {
    pub count: usize,
    pub live: bool,
    pub fields: Vec<Value>,
    pub kind: Kind,
}

/// The counting semantics of §2, keyed by the heap's ids. It trusts
/// the heap for ids only, never for counts.
pub struct Model {
    pub objects: HashMap<ObjId, Shadow>,
    /// Counts the program's bindings hold on each object.
    pub owned: HashMap<ObjId, usize>,
    /// Every id ever allocated, in allocation order.
    pub ids: Vec<ObjId>,
}

impl Model {
    pub fn new() -> Model {
        Model {
            objects: HashMap::new(),
            owned: HashMap::new(),
            ids: Vec::new(),
        }
    }

    pub fn shadow(&mut self, id: ObjId) -> &mut Shadow {
        self.objects.get_mut(&id).expect("model knows every id")
    }

    pub fn alloc(&mut self, id: ObjId, kind: Kind, fields: Vec<Value>) {
        let refs: Vec<ObjId> = fields.iter().filter_map(|v| v.as_ref()).collect();
        self.objects.insert(
            id,
            Shadow {
                count: 1,
                live: true,
                fields,
                kind,
            },
        );
        self.owned.insert(id, 1);
        self.ids.push(id);
        for target in refs {
            self.shadow(target).count += 1;
        }
    }

    pub fn retain(&mut self, id: ObjId) {
        self.shadow(id).count += 1;
        *self.owned.entry(id).or_insert(0) += 1;
    }

    /// A binding gives back its count.
    pub fn release_owned(&mut self, id: ObjId) {
        let owned = self.owned.entry(id).or_insert(0);
        assert!(*owned > 0, "the program does not own a count on {id}");
        *owned -= 1;
        self.release(id);
    }

    /// Count -= 1; at zero, free and release every held `Ref` (§2).
    /// Recursive: the program is acyclic and small.
    pub fn release(&mut self, id: ObjId) {
        let shadow = self.shadow(id);
        assert!(
            shadow.live && shadow.count > 0,
            "model over-release of {id}"
        );
        shadow.count -= 1;
        if shadow.count > 0 {
            return;
        }
        shadow.live = false;
        let fields = std::mem::take(&mut shadow.fields);
        for target in fields.iter().filter_map(|v| v.as_ref()) {
            self.release(target);
        }
    }

    pub fn write(&mut self, id: ObjId, value: Value) {
        if let Some(target) = value.as_ref() {
            self.shadow(target).count += 1;
        }
        let old = std::mem::replace(&mut self.shadow(id).fields[0], value);
        if let Some(target) = old.as_ref() {
            self.release(target);
        }
    }

    pub fn upgrade(&mut self, id: ObjId) -> bool {
        if self.shadow(id).live {
            self.retain(id);
            true
        } else {
            false
        }
    }

    pub fn live_ids(&self) -> Vec<ObjId> {
        self.ids
            .iter()
            .copied()
            .filter(|id| self.objects[id].live)
            .collect()
    }

    pub fn live_mutable_ids(&self) -> Vec<ObjId> {
        self.live_ids()
            .into_iter()
            .filter(|id| self.objects[id].kind.is_mutable())
            .collect()
    }

    pub fn owned_ids(&self) -> Vec<ObjId> {
        self.ids
            .iter()
            .copied()
            .filter(|id| self.owned.get(id).copied().unwrap_or(0) > 0)
            .collect()
    }

    /// Live objects allocated before `id`: the only legal `Ref` targets
    /// of a write into `id` if the program is to stay acyclic.
    pub fn older_live_ids(&self, id: ObjId) -> Vec<ObjId> {
        self.ids
            .iter()
            .copied()
            .take_while(|other| *other != id)
            .filter(|other| self.objects[other].live)
            .collect()
    }
}

/// What the trace says about one object.
pub struct Replayed {
    pub count: usize,
    pub live: bool,
}

/// A replay of the trace, checking that it is well formed as it goes.
pub struct Replay {
    pub objects: HashMap<ObjId, Replayed>,
    /// A `Release` to zero was seen; the next event must be its `Free`.
    pub pending_free: Option<ObjId>,
    /// A live `Upgrade` was seen; the next event must be its `Retain`.
    pub pending_retain: Option<ObjId>,
}

impl Replay {
    pub fn new() -> Replay {
        Replay {
            objects: HashMap::new(),
            pending_free: None,
            pending_retain: None,
        }
    }

    fn live(&self, id: ObjId, what: &str) -> Result<&Replayed, String> {
        match self.objects.get(&id) {
            None => Err(format!("{what} of never-allocated {id}")),
            Some(object) if !object.live => Err(format!("{what} of freed {id}")),
            Some(object) => Ok(object),
        }
    }

    pub fn apply(&mut self, event: Event) -> Result<(), String> {
        if let Some(id) = self.pending_free.take() {
            return match event {
                Event::Free { id: freed } if freed == id => {
                    self.objects.get_mut(&id).expect("seen").live = false;
                    Ok(())
                }
                other => Err(format!(
                    "Release of {id} to zero followed by {other:?}, not Free"
                )),
            };
        }
        if let Some(id) = self.pending_retain.take() {
            if !matches!(event, Event::Retain { id: r, .. } if r == id) {
                return Err(format!(
                    "live Upgrade of {id} followed by {event:?}, not Retain"
                ));
            }
        }
        self.apply_plain(event)
    }

    fn apply_plain(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::Alloc { id, .. } => {
                if self.objects.contains_key(&id) {
                    return Err(format!("Alloc of {id} twice"));
                }
                self.objects.insert(
                    id,
                    Replayed {
                        count: 1,
                        live: true,
                    },
                );
            }
            Event::Retain { id, count_after } => {
                let count = self.live(id, "Retain")?.count;
                if count_after != count + 1 {
                    return Err(format!("Retain of {id}: {count} -> {count_after}"));
                }
                self.objects.get_mut(&id).expect("seen").count = count_after;
            }
            Event::Release { id, count_after } => self.apply_release(id, count_after)?,
            Event::Free { id } => {
                return Err(format!("Free of {id} without a Release to zero"));
            }
            Event::Read { id, .. }
            | Event::Write { id, .. }
            | Event::Weak { id }
            | Event::Shared { id } => {
                self.live(id, "access")?;
            }
            Event::Upgrade { id, live } => self.apply_upgrade(id, live)?,
            // The random programs replayed here make only counted heap
            // objects; these events belong to immortal and stack objects
            // and to unique writes, which this model does not cover.
            other @ (Event::AllocImmortal { .. }
            | Event::AllocStack { .. }
            | Event::WriteUnique { .. }
            | Event::ScopeOpen { .. }
            | Event::ScopeEnd { .. }
            | Event::Drop { .. }
            | Event::Immortalised { .. }
            | Event::InitDone) => {
                return Err(format!("{other:?} is outside the counted model"));
            }
        }
        Ok(())
    }

    /// A `Release` must take a live object's count down by exactly one;
    /// at zero the next event must be its `Free`.
    fn apply_release(&mut self, id: ObjId, count_after: usize) -> Result<(), String> {
        let count = self.live(id, "Release")?.count;
        if count == 0 || count_after != count - 1 {
            return Err(format!("Release of {id}: {count} -> {count_after}"));
        }
        self.objects.get_mut(&id).expect("seen").count = count_after;
        if count_after == 0 {
            self.pending_free = Some(id);
        }
        Ok(())
    }

    /// An `Upgrade` must agree with the trace about liveness; a live
    /// one must be followed by its `Retain`.
    fn apply_upgrade(&mut self, id: ObjId, live: bool) -> Result<(), String> {
        let object = self
            .objects
            .get(&id)
            .ok_or_else(|| format!("Upgrade of never-allocated {id}"))?;
        if object.live != live {
            return Err(format!(
                "Upgrade of {id} says live={live}, trace says {}",
                object.live
            ));
        }
        if live {
            self.pending_retain = Some(id);
        }
        Ok(())
    }
}

/// The heap, the model and the replay agree on every object.
pub fn agree(heap: &Heap, model: &Model, replay: &Replay, seed: u64, step: usize) {
    for &id in &model.ids {
        let shadow = &model.objects[&id];
        let replayed = &replay.objects[&id];
        assert_eq!(
            heap.is_live(id),
            shadow.live,
            "seed {seed} step {step}: heap and model disagree on liveness of {id}"
        );
        assert_eq!(
            replayed.live, shadow.live,
            "seed {seed} step {step}: trace and model disagree on liveness of {id}"
        );
        if shadow.live {
            assert_eq!(
                heap.count(id),
                Ok(shadow.count),
                "seed {seed} step {step}: heap and model disagree on the count of {id}"
            );
            assert_eq!(
                replayed.count, shadow.count,
                "seed {seed} step {step}: trace and model disagree on the count of {id}"
            );
        }
    }
}

/// A random field value: a `Ref` to a live object, a `Weak` to any
/// object (live or not), or a scalar.
fn random_value(rng: &mut Lcg, model: &Model, ref_targets: &[ObjId]) -> Value {
    match rng.below(10) {
        0..=3 => rng
            .pick(ref_targets)
            .map(Value::Ref)
            .unwrap_or(Value::Int(-1)),
        4..=5 => rng.pick(&model.ids).map(Value::Weak).unwrap_or(Value::Nil),
        6 => Value::Nil,
        _ => Value::Int(rng.below(1000) as i64),
    }
}

/// One step of a random program. Every step is legal: the heap must
/// answer `Ok`. `Err` is only ever seen after a deliberately injected
/// fault (`fault`), so on `Err` the model is left untouched.
fn step_alloc(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    let kind = match rng.below(20) {
        0..=11 => Kind::Immutable,
        12..=16 => Kind::Cell,
        _ => Kind::Atom,
    };
    let width = if kind.is_mutable() { 1 } else { rng.below(4) };
    let live = model.live_ids();
    let fields: Vec<Value> = (0..width)
        .map(|_| random_value(rng, model, &live))
        .collect();
    let id = heap.alloc(kind, fields.clone())?;
    model.alloc(id, kind, fields);
    Ok(())
}

/// A write into a random live cell. Acyclic programs only store `Ref`s
/// to older objects; cyclic ones may store any live object, the cell
/// itself included.
fn step_write(
    rng: &mut Lcg,
    heap: &mut Heap,
    model: &mut Model,
    cyclic: bool,
) -> Result<(), AuditError> {
    let Some(id) = rng.pick(&model.live_mutable_ids()) else {
        return Ok(());
    };
    let targets = if cyclic {
        model.live_ids()
    } else {
        model.older_live_ids(id)
    };
    let value = random_value(rng, model, &targets);
    heap.write(id, 0, value)?;
    model.write(id, value);
    Ok(())
}

fn step_read(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    let Some(id) = rng.pick(&model.live_ids()) else {
        return Ok(());
    };
    let width = model.objects[&id].fields.len();
    if width == 0 {
        return Ok(());
    }
    let field = rng.below(width);
    let got = heap.read(id, field)?;
    assert_eq!(
        got, model.objects[&id].fields[field],
        "read of {id}.{field}"
    );
    Ok(())
}

fn step_release(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    if let Some(id) = rng.pick(&model.owned_ids()) {
        heap.release(id)?;
        model.release_owned(id);
    }
    Ok(())
}

fn step_retain(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    if let Some(id) = rng.pick(&model.live_ids()) {
        heap.retain(id)?;
        model.retain(id);
    }
    Ok(())
}

fn step_upgrade(rng: &mut Lcg, heap: &mut Heap, model: &mut Model) -> Result<(), AuditError> {
    if let Some(id) = rng.pick(&model.ids) {
        let got = heap.upgrade(id)?;
        let expected = model.upgrade(id);
        assert_eq!(got, expected.then_some(id), "upgrade of {id}");
    }
    Ok(())
}

/// One random legal operation, mirrored in the model when the heap
/// accepts it.
pub fn step_once(
    rng: &mut Lcg,
    heap: &mut Heap,
    model: &mut Model,
    cyclic: bool,
) -> Result<(), AuditError> {
    match rng.below(100) {
        0..=29 => step_alloc(rng, heap, model),
        30..=44 => step_retain(rng, heap, model),
        45..=64 => step_release(rng, heap, model),
        65..=79 => step_write(rng, heap, model, cyclic),
        80..=89 => step_read(rng, heap, model),
        90..=94 => {
            if let Some(id) = rng.pick(&model.live_ids()) {
                let got = heap.weak(id)?;
                assert_eq!(got, Value::Weak(id));
            }
            Ok(())
        }
        _ => step_upgrade(rng, heap, model),
    }
}

/// Feeds the trace's new events to the replay.
pub fn catch_up(heap: &Heap, replay: &mut Replay, seen: &mut usize, seed: u64, step: usize) {
    for event in &heap.trace()[*seen..] {
        replay
            .apply(*event)
            .unwrap_or_else(|why| panic!("seed {seed} step {step}: malformed trace: {why}"));
    }
    *seen = heap.trace().len();
}
