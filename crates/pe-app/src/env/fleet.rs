//! Merge neighbouring track samples before planning archive reads. One sample
//! call owns the chunk plan, so overlapping boats share downloads and decoding.

use super::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::time::{Duration, Instant};

const MAX_SAMPLES: usize = 10_000;
const SPAN_S: i64 = 72 * 3600;

struct Work {
    task: Task,
    target: Target,
    todo: Vec<(usize, Point)>,
    stokes: bool,
    cursor: usize,
    done: usize,
    outcome: Option<Outcome>,
}

impl Work {
    fn finish(&mut self, state: &AppState, outcome: Outcome) -> Result<()> {
        if self.outcome.is_some() {
            return Ok(());
        }
        let outcome = if outcome != Outcome::Gone
            && with_track(state, self.target, |track| {
                track.env_meta.status =
                    status_of(&track.samples, matches!(outcome, Outcome::Failed(_)));
            })?
            .is_none()
        {
            Outcome::Gone
        } else {
            outcome
        };
        let failure = match &outcome {
            Outcome::Failed(message) => Some(vec![self.task.label.clone(), message.clone()]),
            _ => None,
        };
        state.env_jobs.finish(&self.task, failure);
        self.outcome = Some(outcome);
        Ok(())
    }

    fn accept(
        &mut self,
        state: &AppState,
        indices: &[usize],
        found: pe_env::Result<Vec<EnvPoint>>,
        now: i64,
    ) -> Result<()> {
        if state.env_jobs.cancelled(&self.task) {
            return self.finish(state, Outcome::Cancelled);
        }
        let found = match found {
            Ok(found) if found.len() == indices.len() => found,
            Ok(_) => {
                return self.finish(
                    state,
                    Outcome::Failed("The weather archive returned an incomplete batch".to_owned()),
                );
            }
            Err(EnvError::Cancelled) => return self.finish(state, Outcome::Cancelled),
            Err(err) => return self.finish(state, failed(&err)),
        };
        let provenance = Provenance::of(&self.task.archive);
        if with_track(state, self.target, |track| {
            for (&index, env) in indices.iter().zip(&found) {
                ingest(
                    &mut track.samples[index],
                    &mut track.env_meta,
                    env,
                    &provenance,
                    now,
                );
            }
            track.env_meta.status = status_of(&track.samples, false);
        })?
        .is_none()
        {
            return self.finish(state, Outcome::Gone);
        }
        self.done += indices.len();
        state
            .env_jobs
            .advance(&self.task, self.done as f64 / self.todo.len().max(1) as f64);
        if self.done == self.todo.len() {
            self.finish(state, Outcome::Done)?;
        }
        Ok(())
    }
}

pub(super) fn run(
    state: &AppState,
    tasks: &[Task],
    provider: &dyn Provider,
    sink: &dyn JobSink,
    cancel: &Arc<AtomicBool>,
    now: i64,
) -> Result<Outcome> {
    let mut work = Vec::new();
    let mut outcomes = Vec::new();
    for task in tasks {
        match gather(state, task, now) {
            Ok(Some((target, todo, stokes))) => work.push(Work {
                task: task.clone(),
                target,
                todo,
                stokes,
                cursor: 0,
                done: 0,
                outcome: None,
            }),
            other => {
                let outcome = match other {
                    Err(err) => Outcome::Failed(err.to_string()),
                    _ => Outcome::Gone,
                };
                state.env_jobs.finish(
                    task,
                    match &outcome {
                        Outcome::Failed(message) => Some(vec![task.label.clone(), message.clone()]),
                        _ => None,
                    },
                );
                outcomes.push(outcome);
            }
        }
    }
    let mut prepared = false;
    let mut last_emit = Instant::now();
    loop {
        for item in &mut work {
            if item.outcome.is_none() {
                if state.env_jobs.cancelled(&item.task) {
                    item.finish(state, Outcome::Cancelled)?;
                } else if item.todo.is_empty() {
                    item.finish(state, Outcome::Done)?;
                }
            }
        }
        let next = work
            .iter()
            .filter(|w| w.outcome.is_none())
            .min_by_key(|w| w.todo[w.cursor].1.t);
        let Some(next) = next else {
            break;
        };
        let options = Options {
            interval: next.task.interval,
            stokes_drift: next.stokes,
            parts: pe_env::Parts::ALL,
        };
        let mut heap = BinaryHeap::new();
        for (i, item) in work.iter().enumerate() {
            if item.outcome.is_none() && item.stokes == options.stokes_drift {
                let start = item.todo[item.cursor].1.t;
                heap.push(Reverse((start, i, start)));
            }
        }
        let mut points = Vec::new();
        // Each track's original sample index and its index in the merged result.
        let mut routes: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        while let Some(Reverse((_, i, start))) = heap.pop() {
            if points.len() == MAX_SAMPLES {
                break;
            }
            let item = &mut work[i];
            let (index, point) = item.todo[item.cursor];
            routes.entry(i).or_default().push((index, points.len()));
            points.push(point);
            item.cursor += 1;
            // Bound each route's lookahead, while allowing boats from different
            // dates/races to fill the same shared download pipeline.
            if item.cursor < item.todo.len()
                && item.todo[item.cursor].1.t.saturating_sub(start) < SPAN_S
            {
                heap.push(Reverse((item.todo[item.cursor].1.t, i, start)));
            }
        }
        if !prepared {
            if let Err(err) = provider.prepare(&points, &options, cancel) {
                for item in &mut work {
                    let outcome = if state.env_jobs.cancelled(&item.task)
                        || matches!(err, EnvError::Cancelled)
                    {
                        Outcome::Cancelled
                    } else {
                        failed(&err)
                    };
                    item.finish(state, outcome)?;
                }
                break;
            }
            prepared = true;
        }
        let found = provider.sample(&points, &options, cancel);
        let warnings = provider.take_warnings();
        match found {
            Err(err @ (EnvError::Open(_) | EnvError::Cache(_))) => {
                // Authentication, connection and local-cache failures are
                // shared. Do not repeat a failing request for every boat.
                for item in &mut work {
                    let outcome = if state.env_jobs.cancelled(&item.task) {
                        Outcome::Cancelled
                    } else {
                        failed(&err)
                    };
                    item.finish(state, outcome)?;
                }
            }
            Ok(found) if found.len() == points.len() => {
                for (i, route) in &routes {
                    let indices: Vec<_> = route.iter().map(|(index, _)| *index).collect();
                    let values = route.iter().map(|(_, k)| found[*k]).collect();
                    work[*i].accept(state, &indices, Ok(values), now)?;
                }
            }
            Err(err)
                if routes.len() > 1
                    && !cancel.load(Ordering::SeqCst)
                    && !matches!(err, EnvError::Cancelled) =>
            {
                // A bad chunk for one boat must not fail unrelated boats.
                // Retry bounded subsets; successful chunks are already cached.
                for (i, route) in &routes {
                    let item = &mut work[*i];
                    if state.env_jobs.cancelled(&item.task) {
                        item.finish(state, Outcome::Cancelled)?;
                        continue;
                    }
                    let subset: Vec<_> = route.iter().map(|(_, k)| points[*k]).collect();
                    let indices: Vec<_> = route.iter().map(|(index, _)| *index).collect();
                    item.accept(
                        state,
                        &indices,
                        provider.sample(&subset, &options, cancel),
                        now,
                    )?;
                    state.env_jobs.warn(
                        &item.task.label,
                        provider.take_warnings().into_iter().collect(),
                    );
                }
            }
            other => {
                for i in routes.keys() {
                    let outcome = if state.env_jobs.cancelled(&work[*i].task)
                        || matches!(other, Err(EnvError::Cancelled))
                    {
                        Outcome::Cancelled
                    } else {
                        match &other {
                            Err(err) => failed(err),
                            _ => Outcome::Failed(
                                "The weather archive returned an incomplete batch".to_owned(),
                            ),
                        }
                    };
                    work[*i].finish(state, outcome)?;
                }
            }
        }
        for i in routes.keys() {
            state
                .env_jobs
                .warn(&work[*i].task.label, warnings.iter().cloned().collect());
        }
        if last_emit.elapsed() >= Duration::from_millis(100) {
            sink.changed();
            sink.progress(&state.env_jobs.status());
            last_emit = Instant::now();
        }
    }
    sink.changed();
    outcomes.extend(work.into_iter().filter_map(|w| w.outcome));
    // Report the cohort, while each task retains its own status and error.
    Ok(outcomes
        .iter()
        .find(|o| matches!(o, Outcome::Failed(_)))
        .cloned()
        .or_else(|| outcomes.iter().find(|o| **o == Outcome::Cancelled).cloned())
        .unwrap_or_else(|| {
            if outcomes.iter().all(|o| *o == Outcome::Gone) {
                Outcome::Gone
            } else {
                Outcome::Done
            }
        }))
}
