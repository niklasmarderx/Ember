//! Plugin hook pipeline for tool lifecycle interception.
//!
//! Plugins can register handlers for three events in a tool's lifecycle:
//!
//! - [`HookEvent::PreToolUse`] — fires before execution; a handler may deny the call.
//! - [`HookEvent::PostToolUse`] — fires after successful execution; a handler may
//!   replace the output.
//! - [`HookEvent::PostToolUseFailure`] — fires after a failed execution; useful for
//!   error handling and logging.
//!
//! Handlers run in ascending [`HookHandler::priority`] order (lower number = runs
//! first). If *any* handler returns a denied result the whole [`HookRunner::run`]
//! call is considered denied; messages from all handlers are always collected.
//!
//! # Example
//!
//! ```rust
//! use ember_plugins::hooks::{
//!     HookContext, HookEvent, HookHandler, HookRunResult, HookRunner,
//! };
//!
//! let mut runner = HookRunner::new();
//!
//! runner.register(HookHandler {
//!     name: "logger".to_string(),
//!     events: vec![HookEvent::PreToolUse, HookEvent::PostToolUse],
//!     priority: 0,
//!     handler: Box::new(|ctx| {
//!         HookRunResult::allow_with_messages(vec![
//!             format!("tool '{}' triggered {:?}", ctx.tool_name, ctx.event),
//!         ])
//!     }),
//! });
//!
//! let ctx = HookContext {
//!     event: HookEvent::PreToolUse,
//!     tool_name: "shell".to_string(),
//!     tool_input: r#"{"cmd":"ls"}"#.to_string(),
//!     tool_output: None,
//!     error: None,
//! };
//!
//! let result = runner.run(&ctx);
//! assert!(!result.is_denied());
//! assert_eq!(result.messages().len(), 1);
//! ```

// ---------------------------------------------------------------------------
// HookEvent
// ---------------------------------------------------------------------------

/// Events in a tool's execution lifecycle that plugins can intercept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    /// Fires before a tool is executed.
    ///
    /// A handler returning [`HookRunResult::deny`] will prevent the tool from
    /// running.
    PreToolUse,

    /// Fires after a tool completes successfully.
    ///
    /// A handler may replace the tool output via
    /// [`HookRunResult::modify_output`].
    PostToolUse,

    /// Fires after a tool execution fails.
    ///
    /// Useful for error logging, fallback logic, or alerting.
    PostToolUseFailure,
}

// ---------------------------------------------------------------------------
// HookContext
// ---------------------------------------------------------------------------

/// All information available to a hook handler when it is invoked.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Which lifecycle event triggered this hook.
    pub event: HookEvent,

    /// Name of the tool being (or that was) executed.
    pub tool_name: String,

    /// JSON-encoded input that was (or would be) passed to the tool.
    pub tool_input: String,

    /// JSON-encoded output produced by the tool.
    ///
    /// Only populated for [`HookEvent::PostToolUse`] and
    /// [`HookEvent::PostToolUseFailure`].
    pub tool_output: Option<String>,

    /// Error message from a failed tool execution.
    ///
    /// Only populated for [`HookEvent::PostToolUseFailure`].
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// HookRunResult
// ---------------------------------------------------------------------------

/// The outcome produced by a single hook handler (or by combining multiple).
///
/// Use the constructor methods rather than building the struct directly so that
/// invariants are clear at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRunResult {
    /// Whether execution should be (or was) denied.
    pub denied: bool,

    /// A replacement output string, if the handler wants to modify the tool
    /// output.  Only meaningful for [`HookEvent::PostToolUse`].
    pub modified_output: Option<String>,

    /// Human-readable messages emitted by the handler (log lines, feedback,
    /// denial reasons, …).
    pub messages: Vec<String>,
}

impl HookRunResult {
    /// Allow execution, with no messages and no output modification.
    #[inline]
    pub fn allow() -> Self {
        Self {
            denied: false,
            modified_output: None,
            messages: Vec::new(),
        }
    }

    /// Allow execution and attach diagnostic messages.
    #[inline]
    pub fn allow_with_messages(messages: Vec<String>) -> Self {
        Self {
            denied: false,
            modified_output: None,
            messages,
        }
    }

    /// Deny execution with a human-readable reason.
    ///
    /// The reason is stored as the first message so callers can retrieve it
    /// via [`Self::messages`].
    #[inline]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            denied: true,
            modified_output: None,
            messages: vec![reason.into()],
        }
    }

    /// Allow execution and replace the tool output with `output`.
    #[inline]
    pub fn modify_output(output: String) -> Self {
        Self {
            denied: false,
            modified_output: Some(output),
            messages: Vec::new(),
        }
    }

    /// Returns `true` if this result denies the tool call.
    #[inline]
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    /// Returns the messages attached to this result.
    #[inline]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

// ---------------------------------------------------------------------------
// HookHandler
// ---------------------------------------------------------------------------

/// A named, prioritised handler registered with a [`HookRunner`].
pub struct HookHandler {
    /// Unique name used for registration and removal.
    pub name: String,

    /// The set of events this handler subscribes to.  The handler is only
    /// called when [`HookContext::event`] is present in this list.
    pub events: Vec<HookEvent>,

    /// Execution priority.  Handlers with *lower* values run *first*.
    /// Handlers with equal priority run in registration order.
    pub priority: i32,

    /// The closure invoked for each matching event.
    pub handler: Box<dyn Fn(&HookContext) -> HookRunResult + Send + Sync>,
}

// Manual Debug impl because the closure is not Debug.
impl std::fmt::Debug for HookHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookHandler")
            .field("name", &self.name)
            .field("events", &self.events)
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// HookRunner
// ---------------------------------------------------------------------------

/// Manages a set of [`HookHandler`]s and runs them against a [`HookContext`].
///
/// Handlers are executed in ascending priority order (ties broken by
/// registration order).  All handlers that subscribe to the event are
/// called even if an earlier one returned denied, so that every handler
/// gets a chance to emit messages.
#[derive(Debug, Default)]
pub struct HookRunner {
    handlers: Vec<HookHandler>,
}

impl HookRunner {
    /// Create an empty `HookRunner`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new handler.
    ///
    /// Handlers are stored in insertion order; [`run`](Self::run) sorts them
    /// by priority at call time, so registration order only matters among
    /// handlers with equal priority.
    pub fn register(&mut self, handler: HookHandler) {
        self.handlers.push(handler);
    }

    /// Remove the handler with the given name, if it exists.
    ///
    /// Does nothing if no handler with that name is registered.
    pub fn unregister(&mut self, name: &str) {
        self.handlers.retain(|h| h.name != name);
    }

    /// Run all handlers that subscribe to `context.event`.
    ///
    /// Handlers are called in ascending [`HookHandler::priority`] order.
    /// The returned [`HookRunResult`] is the aggregation of all individual
    /// results:
    ///
    /// - `denied`: `true` if *any* handler denied.
    /// - `modified_output`: the last non-`None` modified output (later
    ///   handlers can chain-transform the output).
    /// - `messages`: all messages from all handlers, in execution order.
    pub fn run(&self, context: &HookContext) -> HookRunResult {
        // Collect references to handlers that care about this event, sorted by priority.
        let mut matching: Vec<&HookHandler> = self
            .handlers
            .iter()
            .filter(|h| h.events.contains(&context.event))
            .collect();

        // stable_sort preserves registration order for equal priorities.
        matching.sort_by_key(|h| h.priority);

        let mut denied = false;
        let mut modified_output: Option<String> = None;
        let mut messages: Vec<String> = Vec::new();

        for handler in matching {
            let result = (handler.handler)(context);

            if result.denied {
                denied = true;
            }
            if result.modified_output.is_some() {
                modified_output = result.modified_output;
            }
            messages.extend(result.messages);
        }

        HookRunResult {
            denied,
            modified_output,
            messages,
        }
    }

    /// Total number of registered handlers (across all events).
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// All handlers that subscribe to `event`, in registration order.
    pub fn handlers_for_event(&self, event: HookEvent) -> Vec<&HookHandler> {
        self.handlers
            .iter()
            .filter(|h| h.events.contains(&event))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers -------------------------------------------------------------------

    fn allow_handler(name: &str, priority: i32, events: Vec<HookEvent>) -> HookHandler {
        let name = name.to_string();
        HookHandler {
            name: name.clone(),
            events,
            priority,
            handler: Box::new(move |_ctx| {
                HookRunResult::allow_with_messages(vec![format!("{name}: allow")])
            }),
        }
    }

    fn deny_handler(name: &str, priority: i32, events: Vec<HookEvent>) -> HookHandler {
        let name = name.to_string();
        HookHandler {
            name: name.clone(),
            events,
            priority,
            handler: Box::new(move |_ctx| HookRunResult::deny(format!("{name}: denied"))),
        }
    }

    fn pre_ctx() -> HookContext {
        HookContext {
            event: HookEvent::PreToolUse,
            tool_name: "shell".to_string(),
            tool_input: r#"{"cmd":"ls"}"#.to_string(),
            tool_output: None,
            error: None,
        }
    }

    fn post_ctx() -> HookContext {
        HookContext {
            event: HookEvent::PostToolUse,
            tool_name: "shell".to_string(),
            tool_input: r#"{"cmd":"ls"}"#.to_string(),
            tool_output: Some("file1\nfile2".to_string()),
            error: None,
        }
    }

    fn failure_ctx() -> HookContext {
        HookContext {
            event: HookEvent::PostToolUseFailure,
            tool_name: "shell".to_string(),
            tool_input: r#"{"cmd":"rm -rf /"}"#.to_string(),
            tool_output: None,
            error: Some("permission denied".to_string()),
        }
    }

    // Tests ---------------------------------------------------------------------

    /// An empty runner must allow execution and produce no messages.
    #[test]
    fn empty_runner_returns_allow() {
        let runner = HookRunner::new();
        let result = runner.run(&pre_ctx());
        assert!(!result.is_denied());
        assert!(result.messages().is_empty());
        assert!(result.modified_output.is_none());
    }

    /// A single handler that allows must produce an allow result.
    #[test]
    fn single_allow_hook() {
        let mut runner = HookRunner::new();
        runner.register(allow_handler("a", 0, vec![HookEvent::PreToolUse]));

        let result = runner.run(&pre_ctx());
        assert!(!result.is_denied());
        assert_eq!(result.messages(), &["a: allow"]);
    }

    /// A single handler that denies must produce a denied result.
    #[test]
    fn single_deny_hook_blocks() {
        let mut runner = HookRunner::new();
        runner.register(deny_handler("guard", 0, vec![HookEvent::PreToolUse]));

        let result = runner.run(&pre_ctx());
        assert!(result.is_denied());
    }

    /// Among multiple handlers, one denial makes the overall result denied.
    #[test]
    fn multiple_hooks_one_deny_is_denied_overall() {
        let mut runner = HookRunner::new();
        runner.register(allow_handler("logger", 0, vec![HookEvent::PreToolUse]));
        runner.register(deny_handler("policy", 10, vec![HookEvent::PreToolUse]));
        runner.register(allow_handler("audit", 20, vec![HookEvent::PreToolUse]));

        let result = runner.run(&pre_ctx());
        assert!(result.is_denied());
        // Messages from all three handlers should be collected.
        assert_eq!(result.messages().len(), 3);
    }

    /// Handlers with lower priority run before those with higher priority.
    #[test]
    fn priority_ordering_lower_runs_first() {
        let mut runner = HookRunner::new();
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

        for (name, prio) in [("high-100", 100), ("low-0", 0), ("mid-50", 50)] {
            let order_clone = std::sync::Arc::clone(&order);
            let label = name.to_string();
            runner.register(HookHandler {
                name: name.to_string(),
                events: vec![HookEvent::PreToolUse],
                priority: prio,
                handler: Box::new(move |_ctx| {
                    order_clone.lock().unwrap().push(label.clone());
                    HookRunResult::allow()
                }),
            });
        }

        runner.run(&pre_ctx());

        let run_order = order.lock().unwrap().clone();
        assert_eq!(run_order, vec!["low-0", "mid-50", "high-100"]);
    }

    /// `PreToolUse` handlers can deny tool execution.
    #[test]
    fn pre_tool_use_can_deny() {
        let mut runner = HookRunner::new();
        runner.register(HookHandler {
            name: "blocker".to_string(),
            events: vec![HookEvent::PreToolUse],
            priority: 0,
            handler: Box::new(|ctx| {
                if ctx.tool_name == "shell" {
                    HookRunResult::deny("shell tool is not allowed")
                } else {
                    HookRunResult::allow()
                }
            }),
        });

        let result = runner.run(&pre_ctx());
        assert!(result.is_denied());
    }

    /// `PostToolUse` handlers can replace the tool output.
    #[test]
    fn post_tool_use_can_modify_output() {
        let mut runner = HookRunner::new();
        runner.register(HookHandler {
            name: "redactor".to_string(),
            events: vec![HookEvent::PostToolUse],
            priority: 0,
            handler: Box::new(|_ctx| HookRunResult::modify_output("[REDACTED]".to_string())),
        });

        let result = runner.run(&post_ctx());
        assert!(!result.is_denied());
        assert_eq!(result.modified_output.as_deref(), Some("[REDACTED]"));
    }

    /// `PostToolUseFailure` handlers receive the error string.
    #[test]
    fn post_tool_use_failure_receives_error() {
        let mut runner = HookRunner::new();
        runner.register(HookHandler {
            name: "error-logger".to_string(),
            events: vec![HookEvent::PostToolUseFailure],
            priority: 0,
            handler: Box::new(|ctx| {
                let msg = format!("caught error: {}", ctx.error.as_deref().unwrap_or("none"));
                HookRunResult::allow_with_messages(vec![msg])
            }),
        });

        let result = runner.run(&failure_ctx());
        assert!(!result.is_denied());
        assert_eq!(result.messages(), &["caught error: permission denied"]);
    }

    /// Messages from all executed handlers are collected in order.
    #[test]
    fn messages_collected_from_all_hooks() {
        let mut runner = HookRunner::new();
        for i in 0..4_i32 {
            let msg = format!("handler-{i}");
            runner.register(HookHandler {
                name: msg.clone(),
                events: vec![HookEvent::PreToolUse],
                priority: i,
                handler: Box::new(move |_ctx| {
                    HookRunResult::allow_with_messages(vec![msg.clone()])
                }),
            });
        }

        let result = runner.run(&pre_ctx());
        assert_eq!(
            result.messages(),
            &["handler-0", "handler-1", "handler-2", "handler-3"]
        );
    }

    /// After unregistering a handler it must no longer be called.
    #[test]
    fn unregister_removes_handler() {
        let mut runner = HookRunner::new();
        runner.register(deny_handler("temporary", 0, vec![HookEvent::PreToolUse]));
        assert_eq!(runner.handler_count(), 1);

        runner.unregister("temporary");
        assert_eq!(runner.handler_count(), 0);

        let result = runner.run(&pre_ctx());
        assert!(!result.is_denied());
    }

    /// `handlers_for_event` must only return handlers subscribed to the given event.
    #[test]
    fn handlers_for_event_filters_correctly() {
        let mut runner = HookRunner::new();

        runner.register(allow_handler(
            "pre-only",
            0,
            vec![HookEvent::PreToolUse],
        ));
        runner.register(allow_handler(
            "post-only",
            0,
            vec![HookEvent::PostToolUse],
        ));
        runner.register(allow_handler(
            "both",
            0,
            vec![HookEvent::PreToolUse, HookEvent::PostToolUse],
        ));

        let pre_handlers = runner.handlers_for_event(HookEvent::PreToolUse);
        assert_eq!(pre_handlers.len(), 2);
        assert!(pre_handlers.iter().any(|h| h.name == "pre-only"));
        assert!(pre_handlers.iter().any(|h| h.name == "both"));

        let post_handlers = runner.handlers_for_event(HookEvent::PostToolUse);
        assert_eq!(post_handlers.len(), 2);
        assert!(post_handlers.iter().any(|h| h.name == "post-only"));
        assert!(post_handlers.iter().any(|h| h.name == "both"));

        let failure_handlers = runner.handlers_for_event(HookEvent::PostToolUseFailure);
        assert!(failure_handlers.is_empty());
    }

    /// A handler not subscribed to an event must not run for that event.
    #[test]
    fn handler_only_runs_for_subscribed_events() {
        let mut runner = HookRunner::new();
        runner.register(deny_handler(
            "post-denier",
            0,
            vec![HookEvent::PostToolUse],
        ));

        // PreToolUse — the post-only handler must not fire.
        let result = runner.run(&pre_ctx());
        assert!(!result.is_denied());

        // PostToolUse — now it must fire.
        let result = runner.run(&post_ctx());
        assert!(result.is_denied());
    }

    /// The last `modify_output` in priority order wins when multiple handlers
    /// each return an output modification.
    #[test]
    fn last_modifier_wins_for_output() {
        let mut runner = HookRunner::new();
        runner.register(HookHandler {
            name: "first".to_string(),
            events: vec![HookEvent::PostToolUse],
            priority: 0,
            handler: Box::new(|_ctx| HookRunResult::modify_output("first-output".to_string())),
        });
        runner.register(HookHandler {
            name: "second".to_string(),
            events: vec![HookEvent::PostToolUse],
            priority: 10,
            handler: Box::new(|_ctx| HookRunResult::modify_output("second-output".to_string())),
        });

        let result = runner.run(&post_ctx());
        assert_eq!(result.modified_output.as_deref(), Some("second-output"));
    }
}
