use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "note_category", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum NoteCategory {
    Light,
    Neutral,
    Heavy,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Note {
    pub id: Uuid,
    pub text: String,
    pub category: NoteCategory,
    pub valence: f32, // PostgreSQL REAL
    pub resonances: i32, // PostgreSQL INTEGER
    pub created_at: DateTime<Utc>,
}

// Wat de client te zien krijgt (strikt minimalistisch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicNoteDto {
    pub id: String,
    pub t: String,
}

impl From<Note> for PublicNoteDto {
    fn from(n: Note) -> Self {
        Self {
            id: n.id.to_string(),
            t: n.text,
        }
    }
}

impl From<&Note> for PublicNoteDto {
    fn from(n: &Note) -> Self {
        Self {
            id: n.id.to_string(),
            t: n.text.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
pub struct FeedQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub seed: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct CreateVentRequest {
    pub text: String,
    #[serde(default)]
    pub website: Option<String>,
}