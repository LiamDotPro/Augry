//! Decides which engine runs which rules. Pure: the check that an engine
//! answers is passed in, so every case is a unit test.

use std::path::PathBuf;

use crate::logic::{Armed, Engine};
use crate::os::Probe;
use crate::rule::EngineChoice;

/// The rules one engine child will serve.
#[derive(Debug)]
pub struct Group {
    pub engine: Engine,
    pub path: PathBuf,
    pub rules: Vec<Armed>,
}

/// Asks `probe` about each engine at most once.
struct Probes<'a> {
    probe: &'a dyn Fn(Engine) -> Probe,
    seen: Vec<(Engine, Probe)>,
}

impl Probes<'_> {
    fn get(&mut self, engine: Engine) -> Probe {
        if let Some((_, known)) = self.seen.iter().find(|(e, _)| *e == engine) {
            return known.clone();
        }
        let found = (self.probe)(engine);
        self.seen.push((engine, found.clone()));
        found
    }
}

/// What the Settings page shows when the engine is there but silent.
fn silent_message(engine: Engine, path: &std::path::Path) -> String {
    format!(
        "{} found at {} but the engine did not answer",
        engine.as_str(),
        path.display()
    )
}

/// Why one named engine cannot be used.
fn explain(engine: Engine, probe: &Probe) -> String {
    match probe {
        Probe::Silent(path) => silent_message(engine, path),
        _ => format!("{} was not found", engine.as_str()),
    }
}

/// Groups the rules by the engine they will use. `auto` takes docker if it
/// answers, else podman. Any rule that cannot get an engine fails the whole
/// plan with the reason, so Settings can show it.
pub fn plan(armed: Vec<Armed>, probe: &dyn Fn(Engine) -> Probe) -> Result<Vec<Group>, String> {
    let mut probes = Probes {
        probe,
        seen: Vec::new(),
    };
    let mut groups: Vec<Group> = Vec::new();
    for rule in armed {
        let (engine, path) = match rule.when.when.engine {
            EngineChoice::Docker => ready(&mut probes, Engine::Docker)?,
            EngineChoice::Podman => ready(&mut probes, Engine::Podman)?,
            EngineChoice::Auto => ready_auto(&mut probes)?,
        };
        match groups.iter_mut().find(|g| g.engine == engine) {
            Some(group) => group.rules.push(rule),
            None => groups.push(Group {
                engine,
                path,
                rules: vec![rule],
            }),
        }
    }
    Ok(groups)
}

fn ready(probes: &mut Probes, engine: Engine) -> Result<(Engine, PathBuf), String> {
    match probes.get(engine) {
        Probe::Ready(path) => Ok((engine, path)),
        other => Err(explain(engine, &other)),
    }
}

fn ready_auto(probes: &mut Probes) -> Result<(Engine, PathBuf), String> {
    let mut reasons = Vec::new();
    for engine in [Engine::Docker, Engine::Podman] {
        match probes.get(engine) {
            Probe::Ready(path) => return Ok((engine, path)),
            Probe::Silent(path) => reasons.push(silent_message(engine, &path)),
            Probe::Missing => {}
        }
    }
    if reasons.is_empty() {
        Err("docker and podman were not found".to_string())
    } else {
        Err(reasons.join("; "))
    }
}

#[cfg(test)]
mod tests;
