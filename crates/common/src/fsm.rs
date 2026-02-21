use crate::types::ComponentState;

/// Returns true if transitioning from `from` to `to` is valid per the FSM rules.
///
/// Valid transitions:
/// - Unknown → Running, Stopped, Failed (first check received)
/// - Stopped → Starting (start command)
/// - Starting → Running (check OK), Failed (timeout or check KO)
/// - Running → Degraded (exit 1), Failed (exit >= 2), Stopping (stop command)
/// - Degraded → Running (exit 0), Failed (exit >= 2), Stopping (stop command)
/// - Stopping → Stopped (stop confirmed)
/// - Failed → Starting (retry), Stopping (cleanup)
/// - Any → Unreachable (heartbeat timeout)
/// - Unreachable → any previous state (agent reconnects)
pub fn is_valid_transition(from: ComponentState, to: ComponentState) -> bool {
    use ComponentState::*;

    // Any state → Unreachable (heartbeat timeout)
    if to == Unreachable && from != Unreachable {
        return true;
    }

    // Unreachable → any previous state (agent reconnects)
    if from == Unreachable && to != Unreachable {
        return true;
    }

    matches!(
        (from, to),
        // Unknown → first check determines state
        (Unknown, Running) | (Unknown, Stopped) | (Unknown, Failed) |
        // Stopped → Starting
        (Stopped, Starting) |
        // Starting → Running or Failed
        (Starting, Running) | (Starting, Failed) |
        // Running → Degraded, Failed, Stopping
        (Running, Degraded) | (Running, Failed) | (Running, Stopping) |
        // Degraded → Running, Failed, Stopping
        (Degraded, Running) | (Degraded, Failed) | (Degraded, Stopping) |
        // Stopping → Stopped
        (Stopping, Stopped) |
        // Failed → Starting, Stopping
        (Failed, Starting) | (Failed, Stopping)
    )
}

/// Given a check result exit code, determine the new state (if any transition needed).
/// Returns None if the state should not change.
pub fn next_state_from_check(current: ComponentState, exit_code: i32) -> Option<ComponentState> {
    use ComponentState::*;

    match (current, exit_code) {
        // Unknown: first check determines state
        (Unknown, 0) => Some(Running),
        (Unknown, _) => Some(Failed),

        // Starting: check OK → Running, check KO → Failed
        (Starting, 0) => Some(Running),
        (Starting, _) => Some(Failed),

        // Running: exit 0 = stay, exit 1 = Degraded, exit >= 2 = Failed
        (Running, 0) => None,
        (Running, 1) => Some(Degraded),
        (Running, _) => Some(Failed),

        // Degraded: exit 0 = Running, exit 1 = stay, exit >= 2 = Failed
        (Degraded, 0) => Some(Running),
        (Degraded, 1) => None,
        (Degraded, _) => Some(Failed),

        // Other states: checks don't trigger transitions
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ComponentState::*;

    // ===== Valid transitions =====

    #[test]
    fn test_unknown_to_running() {
        assert!(is_valid_transition(Unknown, Running));
    }

    #[test]
    fn test_unknown_to_stopped() {
        assert!(is_valid_transition(Unknown, Stopped));
    }

    #[test]
    fn test_unknown_to_failed() {
        assert!(is_valid_transition(Unknown, Failed));
    }

    #[test]
    fn test_stopped_to_starting() {
        assert!(is_valid_transition(Stopped, Starting));
    }

    #[test]
    fn test_starting_to_running() {
        assert!(is_valid_transition(Starting, Running));
    }

    #[test]
    fn test_starting_to_failed() {
        assert!(is_valid_transition(Starting, Failed));
    }

    #[test]
    fn test_running_to_degraded() {
        assert!(is_valid_transition(Running, Degraded));
    }

    #[test]
    fn test_running_to_failed() {
        assert!(is_valid_transition(Running, Failed));
    }

    #[test]
    fn test_running_to_stopping() {
        assert!(is_valid_transition(Running, Stopping));
    }

    #[test]
    fn test_degraded_to_running() {
        assert!(is_valid_transition(Degraded, Running));
    }

    #[test]
    fn test_degraded_to_failed() {
        assert!(is_valid_transition(Degraded, Failed));
    }

    #[test]
    fn test_degraded_to_stopping() {
        assert!(is_valid_transition(Degraded, Stopping));
    }

    #[test]
    fn test_stopping_to_stopped() {
        assert!(is_valid_transition(Stopping, Stopped));
    }

    #[test]
    fn test_failed_to_starting() {
        assert!(is_valid_transition(Failed, Starting));
    }

    #[test]
    fn test_failed_to_stopping() {
        assert!(is_valid_transition(Failed, Stopping));
    }

    #[test]
    fn test_any_to_unreachable() {
        for state in [
            Running, Stopped, Starting, Stopping, Failed, Degraded, Unknown,
        ] {
            assert!(
                is_valid_transition(state, Unreachable),
                "{state} → Unreachable should be valid"
            );
        }
    }

    #[test]
    fn test_unreachable_to_previous() {
        for state in [
            Running, Stopped, Starting, Stopping, Failed, Degraded, Unknown,
        ] {
            assert!(
                is_valid_transition(Unreachable, state),
                "Unreachable → {state} should be valid"
            );
        }
    }

    // ===== Invalid transitions =====

    #[test]
    fn test_invalid_running_to_starting() {
        assert!(!is_valid_transition(Running, Starting));
    }

    #[test]
    fn test_invalid_stopped_to_running() {
        assert!(!is_valid_transition(Stopped, Running));
    }

    #[test]
    fn test_invalid_stopped_to_stopped() {
        assert!(!is_valid_transition(Stopped, Stopped));
    }

    #[test]
    fn test_invalid_starting_to_stopping() {
        assert!(!is_valid_transition(Starting, Stopping));
    }

    #[test]
    fn test_invalid_starting_to_degraded() {
        assert!(!is_valid_transition(Starting, Degraded));
    }

    #[test]
    fn test_invalid_stopping_to_running() {
        assert!(!is_valid_transition(Stopping, Running));
    }

    #[test]
    fn test_invalid_stopping_to_starting() {
        assert!(!is_valid_transition(Stopping, Starting));
    }

    #[test]
    fn test_invalid_failed_to_running() {
        assert!(!is_valid_transition(Failed, Running));
    }

    #[test]
    fn test_invalid_failed_to_degraded() {
        assert!(!is_valid_transition(Failed, Degraded));
    }

    #[test]
    fn test_invalid_unknown_to_starting() {
        assert!(!is_valid_transition(Unknown, Starting));
    }

    // ===== next_state_from_check =====

    #[test]
    fn test_check_starting_exit_0_running() {
        assert_eq!(next_state_from_check(Starting, 0), Some(Running));
    }

    #[test]
    fn test_check_running_exit_1_degraded() {
        assert_eq!(next_state_from_check(Running, 1), Some(Degraded));
    }

    #[test]
    fn test_check_running_exit_2_failed() {
        assert_eq!(next_state_from_check(Running, 2), Some(Failed));
    }

    #[test]
    fn test_check_running_exit_0_no_change() {
        assert_eq!(next_state_from_check(Running, 0), None);
    }

    #[test]
    fn test_check_degraded_exit_0_running() {
        assert_eq!(next_state_from_check(Degraded, 0), Some(Running));
    }

    #[test]
    fn test_check_degraded_exit_1_no_change() {
        assert_eq!(next_state_from_check(Degraded, 1), None);
    }

    #[test]
    fn test_check_degraded_exit_2_failed() {
        assert_eq!(next_state_from_check(Degraded, 2), Some(Failed));
    }

    #[test]
    fn test_check_unknown_exit_0_running() {
        assert_eq!(next_state_from_check(Unknown, 0), Some(Running));
    }

    #[test]
    fn test_check_unknown_exit_1_failed() {
        assert_eq!(next_state_from_check(Unknown, 1), Some(Failed));
    }

    #[test]
    fn test_check_stopped_no_change() {
        assert_eq!(next_state_from_check(Stopped, 0), None);
    }
}
