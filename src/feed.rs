use crate::models::{Note, NoteCategory, PublicNoteDto};
use chrono::Timelike;
use chrono_tz::Europe::Amsterdam;

pub const FEED_BROADCAST_CAPACITY: usize = 64;

pub fn interleave_feed(pool: &[Note]) -> Vec<PublicNoteDto> {
    let hour = chrono::Utc::now().with_timezone(&Amsterdam).hour();
    let is_night = hour >= 23 || hour < 6;

    // Behoud de deterministische volgorde uit Postgres (geen shuffle!)
    // We gebruiken VecDeque of een reversed Vec zodat pop() het eerste element pakt
    let mut heavy: Vec<_> = pool
        .iter()
        .filter(|n| n.category == NoteCategory::Heavy)
        .rev()
        .collect();

    let mut light_or_neutral: Vec<_> = pool
        .iter()
        .filter(|n| matches!(n.category, NoteCategory::Light | NoteCategory::Neutral))
        .rev()
        .collect();

    let mut result = Vec::with_capacity(pool.len());

    while !heavy.is_empty() || !light_or_neutral.is_empty() {
        if is_night {
            // 's Nachts: meer ruimte voor heavy (bijv. 2 heavy op 1 light/neutral)
            for _ in 0..2 {
                if let Some(h) = heavy.pop() {
                    result.push(PublicNoteDto::from(h));
                }
            }
            if let Some(ln) = light_or_neutral.pop() {
                result.push(PublicNoteDto::from(ln));
            }
        } else {
            // Overdag: voornamelijk alledaags/luchtig (bijv. 2 light/neutral op 1 heavy)
            for _ in 0..2 {
                if let Some(ln) = light_or_neutral.pop() {
                    result.push(PublicNoteDto::from(ln));
                }
            }
            if let Some(h) = heavy.pop() {
                result.push(PublicNoteDto::from(h));
            }
        }
    }

    result
}