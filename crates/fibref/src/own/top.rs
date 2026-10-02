//! The whole program, unit by unit in checking order (§3.5 step 5f,
//! step 6): each SCC of `defun`s, then the all-owned bodies of those
//! whose value is taken (§8.4), each `impl` method, each `def`
//! initialiser; with the checks that read the finished facts: the
//! escaping capture of an `&` parameter (§6.5, case 18), `:borrow` on a
//! `defun` parameter and on a protocol method (§6.4).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::types::ast::{ExprKind, FunId};
use crate::types::decls::FunDef;
use crate::types::display::Printer;
use crate::types::infer::UnitRef;
use crate::types::TypedProgram;

use super::error::{OwnError, OwnErrorKind};
use super::facts::Facts;
use super::objects::is_object;
use super::program::{BodyKey, BodyOwn, OwnedProgram, Pass};
use super::syntactic::visit;
use super::unit::{decide, Decided, Unit};
use super::walk::{BodySpec, FrameKind, ParamIn};

/// What a dump may watch: after each unit is decided, the keys of its
/// bodies (in the order they were stored) and its finished facts.
pub(crate) type Observer<'o> = &'o mut dyn FnMut(&[BodyKey], &Facts);

/// Runs the ownership pass over a typed program.
pub fn analyse(p: &TypedProgram) -> Result<OwnedProgram, Vec<OwnError>> {
    analyse_observed(p, &mut |_, _| {})
}

/// [`analyse`], telling `watch` about every unit as it is decided; the
/// pass reads nothing back, so the result is the same.
pub(crate) fn analyse_observed(
    p: &TypedProgram,
    watch: Observer<'_>,
) -> Result<OwnedProgram, Vec<OwnError>> {
    let mut prog = OwnedProgram {
        value_taken: super::taken::value_taken(p),
        methods_taken: super::taken::methods_taken(p),
        ..OwnedProgram::default()
    };
    let mut errors = Vec::new();
    for u in &p.units {
        match u {
            UnitRef::Scc(fs) => scc_unit(p, fs, &mut prog, &mut errors, &mut *watch),
            UnitRef::Macro(f) => scc_unit(p, &[*f], &mut prog, &mut errors, &mut *watch),
            UnitRef::ImplMethod(i, m) => {
                method_unit(p, (*i, *m), &mut prog, &mut errors, &mut *watch)
            }
            UnitRef::Def(d) => {
                let spec = BodySpec {
                    kind: FrameKind::Def,
                    params: Vec::new(),
                    declared_borrow: Vec::new(),
                    body: &p.globals.def(*d).init,
                };
                let unit = Unit {
                    bodies: vec![(BodyKey::Def(*d), spec)],
                    scc: Vec::new(),
                    all_owned: false,
                };
                let done = decide(p, &prog.summaries, &unit, Facts::default());
                seen(&mut *watch, &done);
                store(&mut prog, done.bodies);
            }
        }
    }
    if errors.is_empty() {
        Ok(prog)
    } else {
        Err(errors)
    }
}

fn seen(watch: Observer<'_>, done: &Decided) {
    let keys: Vec<BodyKey> = done.bodies.iter().map(|(k, _)| *k).collect();
    watch(&keys, &done.facts);
}

fn store(prog: &mut OwnedProgram, bodies: Vec<(BodyKey, BodyOwn)>) {
    for (k, b) in bodies {
        prog.order.push(k);
        prog.bodies.insert(k, b);
    }
}

fn param_in(p: &TypedProgram, d: &crate::types::decls::ParamDecl, all_owned: bool) -> ParamIn {
    let obj = p
        .binding_types
        .get(&d.binding)
        .is_none_or(|t| is_object(&p.globals, t));
    match (d.amp, obj) {
        (true, _) => ParamIn::Amp,
        (false, false) => ParamIn::Scalar,
        (false, true) => ParamIn::Obj { owned: all_owned },
    }
}

fn fun_spec<'p>(p: &'p TypedProgram, f: FunId, all_owned: bool) -> BodySpec<'p> {
    let def = p.globals.fun(f);
    BodySpec {
        kind: FrameKind::Defun,
        params: def
            .params
            .iter()
            .map(|d| (d.binding, param_in(p, d, all_owned)))
            .collect(),
        declared_borrow: def.params.iter().map(|d| d.borrow).collect(),
        body: &def.body,
    }
}

fn scc_unit(
    p: &TypedProgram,
    fs: &[FunId],
    prog: &mut OwnedProgram,
    errors: &mut Vec<OwnError>,
    watch: Observer<'_>,
) {
    let members: BTreeMap<FunId, Vec<_>> = fs
        .iter()
        .map(|f| {
            (
                *f,
                p.globals.fun(*f).params.iter().map(|d| d.binding).collect(),
            )
        })
        .collect();
    let inferred: BTreeSet<_> = members.values().flatten().copied().collect();
    let done = decide(
        p,
        &prog.summaries,
        &scc_bodies(p, fs, false),
        Facts::new(members.clone(), inferred, &[]),
    );
    seen(&mut *watch, &done);
    for f in fs {
        prog.summaries.insert(*f, done.facts.summary(p, *f));
        borrow_checks(p.globals.fun(*f), &done.facts, errors);
    }
    for (k, body) in &done.bodies {
        if let BodyKey::Fun(f) = k {
            amp_captures(p, p.globals.fun(*f), body, errors);
        }
    }
    store(prog, done.bodies);
    if fs.iter().any(|f| prog.value_taken.contains(f)) {
        let preset: Vec<_> = members.values().flatten().copied().collect();
        let facts = Facts::new(members, BTreeSet::new(), &preset);
        let done = decide(p, &prog.summaries, &scc_bodies(p, fs, true), facts);
        seen(&mut *watch, &done);
        store(prog, done.bodies);
    }
}

/// The bodies of an SCC, or of its all-owned bodies (§8.4).
fn scc_bodies<'p>(p: &'p TypedProgram, fs: &[FunId], all_owned: bool) -> Unit<'p> {
    let key = |f: FunId| {
        if all_owned {
            BodyKey::AllOwned(f)
        } else {
            BodyKey::Fun(f)
        }
    };
    Unit {
        bodies: fs
            .iter()
            .map(|f| (key(*f), fun_spec(p, *f, all_owned)))
            .collect(),
        scc: fs.to_vec(),
        all_owned,
    }
}

/// `parameter p of f is declared :borrow but escapes` (§6.4).
fn borrow_checks(def: &FunDef, facts: &Facts, errors: &mut Vec<OwnError>) {
    for d in &def.params {
        if d.borrow && facts.escapes.contains_key(&d.binding) {
            let msg = format!(
                "parameter {} of {} is declared :borrow but escapes",
                d.name, def.name
            );
            errors.push(OwnError::new(OwnErrorKind::BorrowEscapes, &def.pos, msg));
        }
    }
}

/// `& parameter captured by escaping closure: v in f` (§6.5, case 18),
/// once per parameter.
fn amp_captures(p: &TypedProgram, def: &FunDef, body: &BodyOwn, errors: &mut Vec<OwnError>) {
    let mut lits = Vec::new();
    visit(&def.body, &mut |e| {
        if matches!(e.kind, ExprKind::Fn(_)) {
            lits.push((e.id, e.pos.clone()));
        }
    });
    let mut seen = HashSet::new();
    for (id, pos) in lits {
        let Some(c) = body.closures.get(&id) else {
            continue;
        };
        if c.escaping.is_none() {
            continue;
        }
        for cap in &c.captures {
            if cap.pass == Pass::OwnCell && seen.insert(cap.binding) {
                let msg = format!(
                    "& parameter captured by escaping closure: {} in {}",
                    p.globals.binding(cap.binding).name,
                    def.name
                );
                errors.push(OwnError::new(OwnErrorKind::AmpCaptured, &pos, msg));
            }
        }
    }
}

fn method_unit(
    p: &TypedProgram,
    (i, m): (usize, usize),
    prog: &mut OwnedProgram,
    errors: &mut Vec<OwnError>,
    watch: Observer<'_>,
) {
    let im = &p.globals.instances[i].methods[m];
    let spec = method_spec(p, i, m);
    let preset: Vec<_> = spec
        .params
        .iter()
        .filter(|(_, pin)| *pin == ParamIn::Obj { owned: true })
        .map(|(b, _)| *b)
        .collect();
    let declared = spec.declared_borrow.clone();
    let unit = Unit {
        bodies: vec![(BodyKey::Method(i, m), spec)],
        scc: Vec::new(),
        all_owned: false,
    };
    let facts = Facts::new(BTreeMap::new(), BTreeSet::new(), &preset);
    let done = decide(p, &prog.summaries, &unit, facts);
    seen(&mut *watch, &done);
    for (j, b) in im.params.iter().enumerate() {
        if declared[j] && done.facts.escapes.contains_key(b) {
            errors.push(impl_escapes(p, i, m, *b));
        }
    }
    store(prog, done.bodies);
    if prog.methods_taken.contains(&(i, m)) {
        let done = method_owned(p, i, m, &prog.summaries);
        seen(&mut *watch, &done);
        store(prog, done.bodies);
    }
}

/// The all-owned body of a method implementation used as a value
/// (§8.4): the body decided afresh with every object parameter owned
/// (declared, like an all-owned body's). The declared kinds were
/// checked on the method's own body; this one reports nothing.
fn method_owned(
    p: &TypedProgram,
    i: usize,
    m: usize,
    summaries: &std::collections::HashMap<FunId, super::program::Summary>,
) -> Decided {
    let mut spec = method_spec(p, i, m);
    for (_, pin) in &mut spec.params {
        if let ParamIn::Obj { owned } = pin {
            *owned = true;
        }
    }
    let preset: Vec<_> = spec
        .params
        .iter()
        .filter(|(_, pin)| matches!(pin, ParamIn::Obj { .. }))
        .map(|(b, _)| *b)
        .collect();
    let unit = Unit {
        bodies: vec![(BodyKey::MethodOwned(i, m), spec)],
        scc: Vec::new(),
        all_owned: false,
    };
    let facts = Facts::new(BTreeMap::new(), BTreeSet::new(), &preset);
    decide(p, summaries, &unit, facts)
}

/// A method body with its declared kinds (syntax §3.10).
fn method_spec(p: &TypedProgram, i: usize, m: usize) -> BodySpec<'_> {
    let inst = &p.globals.instances[i];
    let im = &inst.methods[m];
    let md = &p.globals.proto(inst.proto).methods[im.index];
    let obj = |b| {
        p.binding_types
            .get(b)
            .is_none_or(|t| is_object(&p.globals, t))
    };
    let params = im
        .params
        .iter()
        .enumerate()
        .map(|(j, b)| {
            let owned = md.params.get(j).is_some_and(|mp| mp.owned);
            (
                *b,
                if obj(b) {
                    ParamIn::Obj { owned }
                } else {
                    ParamIn::Scalar
                },
            )
        })
        .collect();
    let declared_borrow = (0..im.params.len())
        .map(|j| md.params.get(j).is_some_and(|mp| mp.borrow))
        .collect();
    BodySpec {
        kind: FrameKind::Method,
        params,
        declared_borrow,
        body: &im.body,
    }
}

/// `implementation of P/m for T makes parameter p escape; the protocol
/// declares it :borrow` (§6.4).
fn impl_escapes(p: &TypedProgram, i: usize, m: usize, b: crate::types::ast::BindingId) -> OwnError {
    let inst = &p.globals.instances[i];
    let im = &inst.methods[m];
    let ty = Printer::with_names(&p.globals, &inst.var_names, &[]).ty(&inst.head);
    let proto = p.globals.proto(inst.proto);
    let msg = format!(
        "implementation of {}/{} for {ty} makes parameter {} escape; the protocol declares it :borrow",
        proto.name,
        proto.methods[im.index].name,
        p.globals.binding(b).name
    );
    OwnError::new(OwnErrorKind::ImplEscapes, &im.pos, msg)
}
