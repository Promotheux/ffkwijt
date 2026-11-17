use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Eenvoudige in-memory rate limiter (sliding window).
///
/// Sleutels (bijv. IP-adressen) worden alleen in het geheugen bijgehouden,
/// nooit opgeslagen in de database en nooit gekoppeld aan een bericht of melding.
/// Na een herstart is alles weg.
pub struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Geeft `true` als de actie is toegestaan en registreert hem dan meteen.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        let entry = hits.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.max {
            return false;
        }
        entry.push(now);
        true
    }

    /// Ruimt verlopen sleutels op zodat het geheugen niet groeit.
    pub fn prune(&self) {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        hits.retain(|_, v| {
            v.retain(|t| now.duration_since(*t) < self.window);
            !v.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blokkeert_na_maximum() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(!rl.check("a"));
        assert!(rl.check("b"));
    }

    #[test]
    fn prune_verwijdert_verlopen_sleutels() {
        let rl = RateLimiter::new(1, Duration::from_millis(1));
        assert!(rl.check("a"));
        std::thread::sleep(Duration::from_millis(5));
        rl.prune();
        assert!(rl.hits.lock().unwrap().is_empty());
        assert!(rl.check("a"));
    }
}
