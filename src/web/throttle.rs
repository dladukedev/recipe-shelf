use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use topcoat::{
    context::{Cx, app_context},
    router::request::client_ip,
};

const WINDOW: Duration = Duration::from_secs(300);
const MAX_ATTEMPTS: u8 = 8;
const MAX_ADDRESSES: usize = 4096;

#[derive(Default)]
struct Attempts {
    started: Option<Instant>,
    failed: u8,
    pending: u8,
}

#[derive(Clone, Default)]
pub(super) struct LoginThrottle(Arc<Mutex<HashMap<String, Attempts>>>);

/// Reservations count toward the limit before password verification starts.
/// Dropping a reservation after an infrastructure failure does not count it as
/// an invalid password. The map never grows beyond MAX_ADDRESSES.
pub(super) struct LoginAttempt {
    throttle: LoginThrottle,
    address: String,
    outcome: Option<bool>,
}

impl LoginThrottle {
    fn begin(&self, address: String) -> Result<LoginAttempt, ()> {
        let mut addresses = self.0.lock().unwrap_or_else(|p| p.into_inner());
        addresses.retain(|_, attempts| {
            attempts.pending > 0
                || attempts
                    .started
                    .is_some_and(|start| start.elapsed() < WINDOW)
        });
        if !addresses.contains_key(&address) && addresses.len() >= MAX_ADDRESSES {
            return Err(());
        }
        let attempts = addresses.entry(address.clone()).or_default();
        if attempts.failed.saturating_add(attempts.pending) >= MAX_ATTEMPTS {
            return Err(());
        }
        attempts.started.get_or_insert_with(Instant::now);
        attempts.pending += 1;
        Ok(LoginAttempt {
            throttle: self.clone(),
            address,
            outcome: None,
        })
    }
}

impl LoginAttempt {
    pub(super) fn finish(&mut self, success: bool) {
        self.outcome = Some(success);
    }
}

impl Drop for LoginAttempt {
    fn drop(&mut self) {
        let mut addresses = self.throttle.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(attempts) = addresses.get_mut(&self.address) {
            attempts.pending = attempts.pending.saturating_sub(1);
            match self.outcome {
                Some(true) => attempts.failed = 0,
                Some(false) => attempts.failed = attempts.failed.saturating_add(1),
                None => {}
            }
        }
    }
}

pub(super) fn login_attempt(cx: &Cx) -> Result<LoginAttempt, ()> {
    let address = client_ip(cx)
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".into());
    app_context::<LoginThrottle>(cx).begin(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_limit_concurrent_verification_and_release_on_failure() {
        let throttle = LoginThrottle::default();
        let pending: Vec<_> = (0..8)
            .map(|_| throttle.begin("ip".into()).unwrap())
            .collect();
        assert!(throttle.begin("ip".into()).is_err());
        drop(pending); // infrastructure failures do not consume password attempts
        let mut valid = throttle.begin("ip".into()).unwrap();
        valid.finish(true);
        drop(valid);
        for _ in 0..8 {
            let mut invalid = throttle.begin("ip".into()).unwrap();
            invalid.finish(false);
        }
        assert!(throttle.begin("ip".into()).is_err());
    }

    #[test]
    fn addresses_are_bounded_and_expired_entries_are_reclaimed() {
        let throttle = LoginThrottle::default();
        for i in 0..MAX_ADDRESSES {
            drop(throttle.begin(i.to_string()).unwrap());
        }
        assert!(throttle.begin("new".into()).is_err());
        for attempts in throttle.0.lock().unwrap().values_mut() {
            attempts.started = Some(Instant::now() - WINDOW);
        }
        assert!(throttle.begin("new".into()).is_ok());
        assert_eq!(throttle.0.lock().unwrap().len(), 1);
    }
}
