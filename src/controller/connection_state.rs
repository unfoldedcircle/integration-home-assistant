// Copyright 2026 blackgold9 <214061+blackgold9@users.noreply.github.com>
// SPDX-License-Identifier: MPL-2.0

//! Lifecycle guard for the single Home Assistant WebSocket connection.

#[derive(Debug, Default)]
pub(crate) struct ConnectionLifecycle {
    phase: ConnectionPhase,
    next_attempt: u64,
    desired_connection: bool,
    reconnect_when_closed: bool,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
enum ConnectionPhase {
    #[default]
    Disconnected,
    Connecting(u64),
    Connected(u64),
    Active(u64),
    Disconnecting(u64),
}

/// What the controller must do after an HA connection attempt/client ends.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConnectionAction {
    RetryAfterBackoff,
    ConnectImmediately,
    Stop,
    IgnoreStale,
}

/// Runtime-selectable connection lifecycle used for a cautious rollout.
///
/// Legacy mode intentionally preserves the original flag-based behavior. State-machine mode
/// delegates to the serialized, generation-aware lifecycle implemented for issue #39.
#[derive(Debug)]
pub(crate) enum ConnectionManager {
    Legacy,
    StateMachine(ConnectionLifecycle),
}

impl ConnectionManager {
    pub(crate) fn new(use_state_machine: bool) -> Self {
        if use_state_machine {
            Self::StateMachine(ConnectionLifecycle::default())
        } else {
            Self::Legacy
        }
    }

    pub(crate) fn is_state_machine(&self) -> bool {
        matches!(self, Self::StateMachine(_))
    }

    pub(crate) fn strategy_name(&self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::StateMachine(_) => "state_machine",
        }
    }

    pub(crate) fn begin_connect(&mut self) -> Option<u64> {
        match self {
            Self::Legacy => Some(0),
            Self::StateMachine(lifecycle) => lifecycle.begin_connect(),
        }
    }

    pub(crate) fn client_started(&mut self, attempt: u64) -> bool {
        match self {
            Self::Legacy => true,
            Self::StateMachine(lifecycle) => lifecycle.client_started(attempt),
        }
    }

    pub(crate) fn accepts_active_event(&self, attempt: u64) -> bool {
        match self {
            Self::Legacy => true,
            Self::StateMachine(lifecycle) => lifecycle.accepts_active_event(attempt),
        }
    }

    pub(crate) fn accepts_closed_event(&self, attempt: u64) -> bool {
        match self {
            Self::Legacy => true,
            Self::StateMachine(lifecycle) => lifecycle.accepts_closed_event(attempt),
        }
    }

    pub(crate) fn client_active(&mut self, attempt: u64) -> bool {
        match self {
            Self::Legacy => true,
            Self::StateMachine(lifecycle) => lifecycle.client_active(attempt),
        }
    }

    pub(crate) fn is_usable(&self, client_available: bool) -> bool {
        match self {
            Self::Legacy => client_available,
            Self::StateMachine(lifecycle) => client_available && lifecycle.is_usable(),
        }
    }

    pub(crate) fn connection_failed(&mut self, attempt: u64) -> ConnectionAction {
        match self {
            Self::Legacy => ConnectionAction::RetryAfterBackoff,
            Self::StateMachine(lifecycle) => lifecycle.connection_failed(attempt),
        }
    }

    pub(crate) fn stop(&mut self) {
        if let Self::StateMachine(lifecycle) = self {
            lifecycle.stop();
        }
    }

    pub(crate) fn disconnect(&mut self) {
        if let Self::StateMachine(lifecycle) = self {
            lifecycle.disconnect();
        }
    }

    pub(crate) fn queue_connect_when_closed(&mut self) -> bool {
        match self {
            Self::Legacy => false,
            Self::StateMachine(lifecycle) => lifecycle.queue_connect_when_closed(),
        }
    }

    pub(crate) fn client_closed(&mut self, attempt: u64) -> ConnectionAction {
        match self {
            Self::Legacy => ConnectionAction::RetryAfterBackoff,
            Self::StateMachine(lifecycle) => lifecycle.client_closed(attempt),
        }
    }
}

impl ConnectionLifecycle {
    /// Starts one connection attempt only when no client exists or is closing.
    ///
    /// Calling this records an explicit desire for an HA connection. If a client is
    /// currently closing, [`Self::queue_connect_when_closed`] records the replacement.
    pub(crate) fn begin_connect(&mut self) -> Option<u64> {
        self.desired_connection = true;
        if self.phase != ConnectionPhase::Disconnected {
            return None;
        }

        self.next_attempt += 1;
        self.phase = ConnectionPhase::Connecting(self.next_attempt);
        Some(self.next_attempt)
    }

    /// Marks a TCP/WebSocket attempt as having produced a client actor.
    pub(crate) fn client_started(&mut self, attempt: u64) -> bool {
        match self.phase {
            ConnectionPhase::Connecting(current) if current == attempt => {
                self.phase = ConnectionPhase::Connected(attempt);
                true
            }
            // The client actor can emit its first event before the controller has
            // resumed the connect future. Preserve that already-active transition.
            ConnectionPhase::Active(current) if current == attempt => true,
            _ => false,
        }
    }

    /// Accepts non-close client events from the currently active attempt only.
    pub(crate) fn accepts_active_event(&self, attempt: u64) -> bool {
        matches!(
            self.phase,
            ConnectionPhase::Connecting(current)
                | ConnectionPhase::Connected(current)
                | ConnectionPhase::Active(current)
                if current == attempt
        )
    }

    /// Accepts a terminal close event from an active or explicitly closing attempt.
    pub(crate) fn accepts_closed_event(&self, attempt: u64) -> bool {
        matches!(
            self.phase,
            ConnectionPhase::Connecting(current)
                | ConnectionPhase::Connected(current)
                | ConnectionPhase::Active(current)
                | ConnectionPhase::Disconnecting(current)
                if current == attempt
        )
    }

    /// Marks the client usable after Home Assistant authentication/subscriptions complete.
    pub(crate) fn client_active(&mut self, attempt: u64) -> bool {
        if self.accepts_active_event(attempt) {
            self.phase = ConnectionPhase::Active(attempt);
            true
        } else {
            false
        }
    }

    /// Whether controller business messages may be sent to the HA actor.
    pub(crate) fn is_usable(&self) -> bool {
        matches!(self.phase, ConnectionPhase::Active(_))
    }

    /// Marks a failed connection attempt as ended and returns the required follow-up.
    pub(crate) fn connection_failed(&mut self, attempt: u64) -> ConnectionAction {
        if !matches!(
            self.phase,
            ConnectionPhase::Connecting(current) | ConnectionPhase::Disconnecting(current)
                if current == attempt
        ) {
            return ConnectionAction::IgnoreStale;
        }

        self.phase = ConnectionPhase::Disconnected;
        self.ended_action()
    }

    /// Disables automatic reconnects after an unrecoverable error or retry exhaustion.
    pub(crate) fn stop(&mut self) {
        self.desired_connection = false;
        self.reconnect_when_closed = false;
    }

    /// Starts explicit teardown. A replacement request must wait for this attempt to end.
    pub(crate) fn disconnect(&mut self) {
        self.desired_connection = false;
        self.reconnect_when_closed = false;
        self.phase = match self.phase {
            ConnectionPhase::Connecting(attempt)
            | ConnectionPhase::Connected(attempt)
            | ConnectionPhase::Active(attempt) => ConnectionPhase::Disconnecting(attempt),
            phase => phase,
        };
    }

    /// Queues one explicit replacement connection until the current attempt ends.
    /// Returns false for an active/pending connection, so duplicate Connect events
    /// cannot be mistaken for reconfiguration.
    pub(crate) fn queue_connect_when_closed(&mut self) -> bool {
        if matches!(self.phase, ConnectionPhase::Disconnecting(_)) {
            self.desired_connection = true;
            self.reconnect_when_closed = true;
            true
        } else {
            false
        }
    }

    /// Handles a client stop and returns the required controller follow-up.
    ///
    /// The event handler normally calls this only after `accepts_closed_event`.
    /// This defensive check protects future direct callers from stale close events.
    pub(crate) fn client_closed(&mut self, attempt: u64) -> ConnectionAction {
        if !self.accepts_closed_event(attempt) {
            return ConnectionAction::IgnoreStale;
        }

        self.phase = ConnectionPhase::Disconnected;
        self.ended_action()
    }

    fn ended_action(&mut self) -> ConnectionAction {
        if self.reconnect_when_closed {
            self.reconnect_when_closed = false;
            ConnectionAction::ConnectImmediately
        } else if self.desired_connection {
            ConnectionAction::RetryAfterBackoff
        } else {
            ConnectionAction::Stop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionAction, ConnectionLifecycle, ConnectionManager};

    #[test]
    fn rollout_defaults_to_the_legacy_connection_manager() {
        let mut manager = ConnectionManager::new(false);

        assert_eq!(manager.strategy_name(), "legacy");
        assert_eq!(manager.begin_connect(), Some(0));
        assert_eq!(manager.begin_connect(), Some(0));
    }

    #[test]
    fn rollout_flag_enables_the_serialized_connection_manager() {
        let mut manager = ConnectionManager::new(true);

        assert_eq!(manager.strategy_name(), "state_machine");
        assert!(manager.begin_connect().is_some());
        assert!(manager.begin_connect().is_none());
    }

    #[test]
    fn legacy_and_state_machine_modes_keep_their_original_request_gating() {
        let legacy = ConnectionManager::new(false);
        assert!(legacy.is_usable(true));

        let mut state_machine = ConnectionManager::new(true);
        let attempt = state_machine.begin_connect().unwrap();
        assert!(!state_machine.is_usable(true));
        assert!(state_machine.client_started(attempt));
        assert!(state_machine.client_active(attempt));
        assert!(state_machine.is_usable(true));
    }

    #[test]
    fn issue_39_does_not_start_a_second_connection_while_first_is_pending() {
        let mut lifecycle = ConnectionLifecycle::default();

        assert!(lifecycle.begin_connect().is_some());
        assert!(lifecycle.begin_connect().is_none());
    }

    #[test]
    fn issue_39_reconfiguration_invalidates_an_inflight_connection() {
        let mut lifecycle = ConnectionLifecycle::default();
        let attempt = lifecycle.begin_connect().expect("first connection starts");

        lifecycle.disconnect();

        assert!(!lifecycle.client_started(attempt));
        assert!(lifecycle.begin_connect().is_none());
        assert!(lifecycle.queue_connect_when_closed());
        assert_eq!(
            lifecycle.connection_failed(attempt),
            ConnectionAction::ConnectImmediately
        );
        assert!(lifecycle.begin_connect().is_some());
    }

    #[test]
    fn reconfiguration_waits_for_the_old_client_to_close_before_connecting() {
        let mut lifecycle = ConnectionLifecycle::default();
        let attempt = lifecycle.begin_connect().expect("first connection starts");
        assert!(lifecycle.client_started(attempt));

        lifecycle.disconnect();
        assert!(lifecycle.queue_connect_when_closed());

        assert!(lifecycle.begin_connect().is_none());
        assert_eq!(
            lifecycle.client_closed(attempt),
            ConnectionAction::ConnectImmediately
        );
        assert!(lifecycle.begin_connect().is_some());
    }

    #[test]
    fn connection_timeout_releases_attempt_for_backoff_retry() {
        let mut lifecycle = ConnectionLifecycle::default();
        let attempt = lifecycle.begin_connect().expect("first connection starts");

        assert_eq!(
            lifecycle.connection_failed(attempt),
            ConnectionAction::RetryAfterBackoff
        );
        assert!(lifecycle.begin_connect().is_some());
    }

    #[test]
    fn disconnect_rejects_late_active_event_but_accepts_its_close() {
        let mut lifecycle = ConnectionLifecycle::default();
        let attempt = lifecycle.begin_connect().expect("first connection starts");
        assert!(lifecycle.client_started(attempt));

        lifecycle.disconnect();

        assert!(!lifecycle.accepts_active_event(attempt));
        assert!(lifecycle.accepts_closed_event(attempt));
        assert_eq!(lifecycle.client_closed(attempt), ConnectionAction::Stop);
    }

    #[test]
    fn stale_close_after_a_replacement_attempt_is_ignored() {
        let mut lifecycle = ConnectionLifecycle::default();
        let first = lifecycle.begin_connect().expect("first connection starts");
        assert!(lifecycle.client_started(first));
        lifecycle.disconnect();
        assert!(lifecycle.queue_connect_when_closed());
        assert_eq!(
            lifecycle.client_closed(first),
            ConnectionAction::ConnectImmediately
        );
        let second = lifecycle.begin_connect().expect("replacement starts");

        assert_eq!(
            lifecycle.client_closed(first),
            ConnectionAction::IgnoreStale
        );
        assert!(lifecycle.accepts_active_event(second));
    }
}
