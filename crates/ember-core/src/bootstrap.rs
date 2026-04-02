//! Phase-based bootstrap pipeline for structured startup.
//!
//! Instead of a monolithic startup sequence, each component declares which
//! [`BootstrapPhase`] it belongs to. A [`BootstrapPlan`] defines the set and
//! order of phases to execute, while [`BootstrapTimer`] records per-phase
//! wall-clock durations for diagnostics and optimisation.
//!
//! # Example
//!
//! ```rust
//! use ember_core::bootstrap::{BootstrapPhase, BootstrapPlan, BootstrapTimer};
//!
//! let plan = BootstrapPlan::default_plan();
//! let mut timer = BootstrapTimer::new();
//!
//! for &phase in plan.phases() {
//!     timer.start_phase(phase);
//!     // … execute phase …
//!     timer.end_phase(phase);
//! }
//!
//! println!("{}", timer.format_report());
//! ```

use std::time::{Duration, Instant};

// ─── BootstrapPhase ──────────────────────────────────────────────────────────

/// Ordered startup phases executed during Ember initialisation.
///
/// The variants are listed in their *default* execution order, from the very
/// first CLI entry point all the way to the live REPL loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BootstrapPhase {
    /// Parse CLI arguments and set up structured logging.
    CliEntry,
    /// Load the configuration file and validate all values.
    ConfigLoad,
    /// Establish connections to the configured LLM providers.
    ProviderInit,
    /// Discover and load WASM plugins from the plugin directory.
    PluginDiscovery,
    /// Connect to and verify MCP (Model Context Protocol) servers.
    McpSetup,
    /// Assemble the final system prompt from config + plugins.
    SystemPrompt,
    /// Register all built-in and plugin-supplied tools.
    ToolRegistry,
    /// Start a new conversation session or resume an existing one.
    SessionInit,
    /// Enter the interactive REPL / streaming response loop.
    MainRuntime,
}

impl BootstrapPhase {
    /// Returns a short, human-readable identifier for this phase.
    ///
    /// The string is stable and suitable for use in log messages or reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CliEntry => "CliEntry",
            Self::ConfigLoad => "ConfigLoad",
            Self::ProviderInit => "ProviderInit",
            Self::PluginDiscovery => "PluginDiscovery",
            Self::McpSetup => "McpSetup",
            Self::SystemPrompt => "SystemPrompt",
            Self::ToolRegistry => "ToolRegistry",
            Self::SessionInit => "SessionInit",
            Self::MainRuntime => "MainRuntime",
        }
    }

    /// Returns all phases in their canonical execution order.
    pub fn all_phases() -> Vec<Self> {
        vec![
            Self::CliEntry,
            Self::ConfigLoad,
            Self::ProviderInit,
            Self::PluginDiscovery,
            Self::McpSetup,
            Self::SystemPrompt,
            Self::ToolRegistry,
            Self::SessionInit,
            Self::MainRuntime,
        ]
    }
}

// ─── BootstrapPlan ───────────────────────────────────────────────────────────

/// An ordered list of [`BootstrapPhase`]s to execute during startup.
///
/// A plan is constructed once and then iterated by the startup driver.
/// Phases are kept in the order they were added; duplicates are not allowed.
pub struct BootstrapPlan {
    phases: Vec<BootstrapPhase>,
}

impl BootstrapPlan {
    /// Creates a plan containing all phases in their default execution order.
    pub fn default_plan() -> Self {
        Self {
            phases: BootstrapPhase::all_phases(),
        }
    }

    /// Creates a plan with all default phases *except* those listed in `skip`.
    ///
    /// Useful for fast-path startup where certain expensive phases (e.g.
    /// [`BootstrapPhase::PluginDiscovery`]) are intentionally skipped.
    pub fn fast_path(skip: &[BootstrapPhase]) -> Self {
        let phases = BootstrapPhase::all_phases()
            .into_iter()
            .filter(|p| !skip.contains(p))
            .collect();
        Self { phases }
    }

    /// Creates a plan from an arbitrary list of phases, deduplicating while
    /// preserving the first occurrence of each phase.
    pub fn from_phases(phases: Vec<BootstrapPhase>) -> Self {
        let mut seen = std::collections::HashSet::new();
        let deduped = phases.into_iter().filter(|p| seen.insert(*p)).collect();
        Self { phases: deduped }
    }

    /// Returns a slice of the phases in this plan, in execution order.
    pub fn phases(&self) -> &[BootstrapPhase] {
        &self.phases
    }

    /// Returns `true` if the given phase is present in this plan.
    pub fn contains(&self, phase: BootstrapPhase) -> bool {
        self.phases.contains(&phase)
    }

    /// Inserts `new` immediately *before* the first occurrence of `target`.
    ///
    /// If `target` is not found, `new` is appended to the end.
    /// If `new` already exists in the plan it is not inserted again.
    pub fn insert_before(&mut self, target: BootstrapPhase, new: BootstrapPhase) {
        if self.phases.contains(&new) {
            return;
        }
        match self.phases.iter().position(|&p| p == target) {
            Some(idx) => self.phases.insert(idx, new),
            None => self.phases.push(new),
        }
    }

    /// Inserts `new` immediately *after* the first occurrence of `target`.
    ///
    /// If `target` is not found, `new` is appended to the end.
    /// If `new` already exists in the plan it is not inserted again.
    pub fn insert_after(&mut self, target: BootstrapPhase, new: BootstrapPhase) {
        if self.phases.contains(&new) {
            return;
        }
        match self.phases.iter().position(|&p| p == target) {
            Some(idx) => self.phases.insert(idx + 1, new),
            None => self.phases.push(new),
        }
    }

    /// Removes all occurrences of `phase` from the plan.
    pub fn remove(&mut self, phase: BootstrapPhase) {
        self.phases.retain(|&p| p != phase);
    }
}

// ─── BootstrapTimer ──────────────────────────────────────────────────────────

/// Records wall-clock timing for individual bootstrap phases.
///
/// Call [`start_phase`](BootstrapTimer::start_phase) before executing a phase
/// and [`end_phase`](BootstrapTimer::end_phase) once it completes.
/// Unmatched `end_phase` calls (without a preceding `start_phase`) are
/// silently ignored.
pub struct BootstrapTimer {
    /// Completed phase timings in insertion order.
    phase_times: Vec<(BootstrapPhase, Duration)>,
    /// Wall-clock instant when the timer was created.
    start: Instant,
    /// Pending phase start times (phase → when it was started).
    pending: Vec<(BootstrapPhase, Instant)>,
}

impl BootstrapTimer {
    /// Creates a new timer. The total elapsed clock starts immediately.
    pub fn new() -> Self {
        Self {
            phase_times: Vec::new(),
            start: Instant::now(),
            pending: Vec::new(),
        }
    }

    /// Records the start of `phase`. Must be followed by a matching
    /// [`end_phase`](BootstrapTimer::end_phase) call.
    pub fn start_phase(&mut self, phase: BootstrapPhase) {
        self.pending.push((phase, Instant::now()));
    }

    /// Records the end of `phase` and stores the elapsed duration.
    ///
    /// Uses the most recent unmatched `start_phase` call for the same phase.
    /// No-op if no matching start was recorded.
    pub fn end_phase(&mut self, phase: BootstrapPhase) {
        let now = Instant::now();
        // Find and remove the most recent pending entry for this phase.
        if let Some(pos) = self.pending.iter().rposition(|(p, _)| *p == phase) {
            let (_, started_at) = self.pending.remove(pos);
            self.phase_times
                .push((phase, now.duration_since(started_at)));
        }
    }

    /// Returns the total wall-clock duration since this timer was created.
    pub fn total_elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Formats a one-line timing report.
    ///
    /// Each completed phase is shown as `Name: Xms`, separated by ` | `.
    /// The total elapsed time is appended as `Total: Xms`.
    ///
    /// Example output:
    /// ```text
    /// ConfigLoad: 12ms | ProviderInit: 45ms | Total: 120ms
    /// ```
    pub fn format_report(&self) -> String {
        let mut parts: Vec<String> = self
            .phase_times
            .iter()
            .map(|(phase, dur)| format!("{}: {}ms", phase.as_str(), dur.as_millis()))
            .collect();

        parts.push(format!("Total: {}ms", self.total_elapsed().as_millis()));
        parts.join(" | ")
    }
}

impl Default for BootstrapTimer {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Default plan contains all phases in canonical order.
    #[test]
    fn default_plan_has_all_phases_in_order() {
        let plan = BootstrapPlan::default_plan();
        let expected = BootstrapPhase::all_phases();
        assert_eq!(plan.phases(), expected.as_slice());
    }

    // 2. from_phases deduplicates while keeping the first occurrence.
    #[test]
    fn from_phases_deduplicates() {
        let plan = BootstrapPlan::from_phases(vec![
            BootstrapPhase::ConfigLoad,
            BootstrapPhase::CliEntry,
            BootstrapPhase::ConfigLoad, // duplicate → should be dropped
            BootstrapPhase::ProviderInit,
            BootstrapPhase::CliEntry, // duplicate → should be dropped
        ]);
        assert_eq!(
            plan.phases(),
            &[
                BootstrapPhase::ConfigLoad,
                BootstrapPhase::CliEntry,
                BootstrapPhase::ProviderInit,
            ]
        );
    }

    // 3. fast_path skips the requested phases.
    #[test]
    fn fast_path_skips_phases() {
        let skip = [BootstrapPhase::PluginDiscovery, BootstrapPhase::McpSetup];
        let plan = BootstrapPlan::fast_path(&skip);

        for &skipped in &skip {
            assert!(
                !plan.contains(skipped),
                "{} should be absent",
                skipped.as_str()
            );
        }
        // All other phases must still be present.
        let remaining: Vec<_> = BootstrapPhase::all_phases()
            .into_iter()
            .filter(|p| !skip.contains(p))
            .collect();
        assert_eq!(plan.phases(), remaining.as_slice());
    }

    // 4. contains returns correct results.
    #[test]
    fn contains_is_correct() {
        let plan = BootstrapPlan::fast_path(&[BootstrapPhase::PluginDiscovery]);
        assert!(plan.contains(BootstrapPhase::ConfigLoad));
        assert!(!plan.contains(BootstrapPhase::PluginDiscovery));
    }

    // 5. insert_before places the phase at the right position.
    #[test]
    fn insert_before_places_correctly() {
        let mut plan = BootstrapPlan::from_phases(vec![
            BootstrapPhase::CliEntry,
            BootstrapPhase::ConfigLoad,
            BootstrapPhase::MainRuntime,
        ]);
        plan.insert_before(BootstrapPhase::ConfigLoad, BootstrapPhase::ProviderInit);

        assert_eq!(
            plan.phases(),
            &[
                BootstrapPhase::CliEntry,
                BootstrapPhase::ProviderInit,
                BootstrapPhase::ConfigLoad,
                BootstrapPhase::MainRuntime,
            ]
        );
    }

    // 6. insert_after places the phase at the right position.
    #[test]
    fn insert_after_places_correctly() {
        let mut plan = BootstrapPlan::from_phases(vec![
            BootstrapPhase::CliEntry,
            BootstrapPhase::ConfigLoad,
            BootstrapPhase::MainRuntime,
        ]);
        plan.insert_after(BootstrapPhase::ConfigLoad, BootstrapPhase::ProviderInit);

        assert_eq!(
            plan.phases(),
            &[
                BootstrapPhase::CliEntry,
                BootstrapPhase::ConfigLoad,
                BootstrapPhase::ProviderInit,
                BootstrapPhase::MainRuntime,
            ]
        );
    }

    // 7. remove eliminates the phase from the plan.
    #[test]
    fn remove_eliminates_phase() {
        let mut plan = BootstrapPlan::default_plan();
        assert!(plan.contains(BootstrapPhase::McpSetup));
        plan.remove(BootstrapPhase::McpSetup);
        assert!(!plan.contains(BootstrapPhase::McpSetup));
        // Length should shrink by exactly one.
        assert_eq!(plan.phases().len(), BootstrapPhase::all_phases().len() - 1);
    }

    // 8. Timer tracks phase durations (non-zero for a real sleep).
    #[test]
    fn timer_tracks_phase_durations() {
        let mut timer = BootstrapTimer::new();
        timer.start_phase(BootstrapPhase::ConfigLoad);
        // Spin-wait a tiny bit so the duration is non-zero.
        let deadline = std::time::Instant::now() + Duration::from_millis(2);
        while std::time::Instant::now() < deadline {}
        timer.end_phase(BootstrapPhase::ConfigLoad);

        assert_eq!(timer.phase_times.len(), 1);
        let (phase, dur) = timer.phase_times[0];
        assert_eq!(phase, BootstrapPhase::ConfigLoad);
        assert!(dur >= Duration::from_millis(1));
    }

    // 9. format_report is non-empty and contains phase names.
    #[test]
    fn format_report_is_non_empty_and_contains_phases() {
        let mut timer = BootstrapTimer::new();
        timer.start_phase(BootstrapPhase::CliEntry);
        timer.end_phase(BootstrapPhase::CliEntry);
        timer.start_phase(BootstrapPhase::ConfigLoad);
        timer.end_phase(BootstrapPhase::ConfigLoad);

        let report = timer.format_report();
        assert!(!report.is_empty());
        assert!(report.contains("CliEntry:"), "report = {report}");
        assert!(report.contains("ConfigLoad:"), "report = {report}");
        assert!(report.contains("Total:"), "report = {report}");
    }

    // 10. Empty plan edge case: no phases, report is just "Total: Xms".
    #[test]
    fn empty_plan_is_valid() {
        let plan = BootstrapPlan::from_phases(vec![]);
        assert_eq!(plan.phases().len(), 0);
        assert!(!plan.contains(BootstrapPhase::CliEntry));

        let timer = BootstrapTimer::new();
        let report = timer.format_report();
        assert!(report.starts_with("Total:"), "report = {report}");
    }

    // 11. insert_before on an empty plan appends the phase.
    #[test]
    fn insert_before_on_empty_plan_appends() {
        let mut plan = BootstrapPlan::from_phases(vec![]);
        plan.insert_before(BootstrapPhase::ConfigLoad, BootstrapPhase::CliEntry);
        assert_eq!(plan.phases(), &[BootstrapPhase::CliEntry]);
    }

    // 12. insert_after on an empty plan appends the phase.
    #[test]
    fn insert_after_on_empty_plan_appends() {
        let mut plan = BootstrapPlan::from_phases(vec![]);
        plan.insert_after(BootstrapPhase::ConfigLoad, BootstrapPhase::CliEntry);
        assert_eq!(plan.phases(), &[BootstrapPhase::CliEntry]);
    }

    // 13. Inserting a phase that already exists is a no-op.
    #[test]
    fn insert_existing_phase_is_noop() {
        let mut plan =
            BootstrapPlan::from_phases(vec![BootstrapPhase::CliEntry, BootstrapPhase::ConfigLoad]);
        plan.insert_before(BootstrapPhase::ConfigLoad, BootstrapPhase::CliEntry);
        // Should remain unchanged.
        assert_eq!(
            plan.phases(),
            &[BootstrapPhase::CliEntry, BootstrapPhase::ConfigLoad]
        );
    }

    // 14. total_elapsed increases over time.
    #[test]
    fn total_elapsed_increases() {
        let timer = BootstrapTimer::new();
        let t1 = timer.total_elapsed();
        let deadline = std::time::Instant::now() + Duration::from_millis(2);
        while std::time::Instant::now() < deadline {}
        let t2 = timer.total_elapsed();
        assert!(t2 > t1);
    }

    // 15. end_phase without start_phase is a no-op (no panic, no entry).
    #[test]
    fn end_phase_without_start_is_noop() {
        let mut timer = BootstrapTimer::new();
        timer.end_phase(BootstrapPhase::ProviderInit); // no matching start
        assert_eq!(timer.phase_times.len(), 0);
    }
}
