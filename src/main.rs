mod db;
mod feed;
mod models;
mod moderator;
mod sanitizer;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use models::{CreateVentRequest, FeedQuery, Note, NoteCategory, PublicNoteDto};
use moderator::Moderator;
use sqlx::postgres::PgListener;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: db::DbPool,
    moderator: Arc<Moderator>,
    feed_tx: broadcast::Sender<PublicNoteDto>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL ontbreekt");
    let pool = db::init_db(&db_url).await?;

    let moderator = Arc::new(Moderator::new_embedded("models")?);

    // Channel buffer voor live feed updates naar de browsers
    let (feed_tx, _feed_rx) = broadcast::channel::<PublicNoteDto>(100);

    // Luisteraar voor berichten die later goedgekeurd zijn via ffkwijt-admin
    let approved_pool = pool.clone();
    let approved_tx = feed_tx.clone();
    let approved_db_url = db_url.clone();

    tokio::spawn(async move {
        let mut listener = match PgListener::connect(&approved_db_url).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Fout bij verbinden PgListener (approved): {:?}", e);
                return;
            }
        };

        if let Err(e) = listener.listen("vent_approved").await {
            eprintln!("Kan niet luisteren op vent_approved: {:?}", e);
            return;
        }

        println!("API PgListener actief op Postgres-kanaal 'vent_approved'");

        while let Ok(notification) = listener.recv().await {
            if let Ok(id) = Uuid::parse_str(notification.payload()) {
                let note_res = sqlx::query_as::<_, Note>(
                    r#"
                    SELECT id, text, category, valence, resonances, moderation_status AS status, created_at
                    FROM notes
                    WHERE id = $1 AND moderation_status = 'approved'
                    "#,
                )
                .bind(id)
                .fetch_optional(&approved_pool)
                .await;

                if let Ok(Some(note)) = note_res {
                    let _ = approved_tx.send(PublicNoteDto::from(note));
                }
            }
        }
    });

    let state = AppState {
        db: pool,
        moderator,
        feed_tx,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let serve_dir = ServeDir::new("public");

    let app = Router::new()
        .route("/api/feed", get(get_feed))
        .route("/api/feed/stream", get(stream_feed))
        .route("/api/vent", post(post_vent))
        .route("/api/resonate/:id", post(post_resonate))
        .route_service("/", ServeFile::new("public/index.html"))
        .fallback_service(serve_dir)
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("ffkwijt backend met PostgreSQL luistert op http://127.0.0.1:3000");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_feed(
    State(state): State<AppState>,
    Query(params): Query<FeedQuery>,
) -> Result<Json<Vec<PublicNoteDto>>, (StatusCode, String)> {
    let page = i64::from(params.page.unwrap_or(1).max(1));
    let limit = i64::from(params.limit.unwrap_or(20).clamp(1, 50));
    let offset = (page - 1) * limit;
    let seed = params.seed.unwrap_or_else(|| "ffkwijt".to_string());

    let notes = sqlx::query_as::<_, Note>(
        r#"
        SELECT id, text, category, valence, resonances, moderation_status AS status, created_at 
        FROM notes 
        WHERE moderation_status = 'approved'
        AND (
            created_at >= NOW() - INTERVAL '90 days'
            OR resonances >= 1
        )
        ORDER BY md5(id::text || $1)
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(seed)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|err| {
        eprintln!("Database query fout: {:?}", err);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("DB fout: {err}"))
    })?;

    let interleaved = feed::interleave_feed(&notes);
    Ok(Json(interleaved))
}

// Passieve SSE-verbinding voor browsers
async fn stream_feed(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.feed_tx.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(note) => {
            let json = serde_json::to_string(&note).unwrap_or_default();
            Some(Ok(Event::default().event("new_note").data(json)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn post_vent(
    State(state): State<AppState>,
    Json(payload): Json<CreateVentRequest>,
) -> impl IntoResponse {
    if let Some(ref trap) = payload.website {
        if !trap.trim().is_empty() {
            eprintln!("--> Honeypot geactiveerd: bot genegeerd");
            return (StatusCode::CREATED, "Geplaatst").into_response();
        }
    }
    
    let text = payload.text.trim();
    eprintln!("Nieuwe vent ontvangen voor analyse");

    if text.is_empty() || text.len() > 280 {
        return (StatusCode::BAD_REQUEST, "Ongeldige tekstlengte").into_response();
    }

    if sanitizer::contains_obvious_pii(text) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Bevat herleidbare persoonsgegevens (telefoon, mail of handle)",
        )
            .into_response();
    }

    if sanitizer::is_explicit_crisis_trigger(text) {
        eprintln!("--> Vangrail geraakt: CRISIS_DETECTED");
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "CRISIS_DETECTED",
        )
            .into_response();
    }

    let (category, valence, safety) = match state.moderator.evaluate(text) {
        Ok(eval) => {
            eprintln!(
                "--> AI Resultaat: category={:?}, valence={}, safety=\"{}\"",
                eval.category, eval.valence, eval.safety
            );

            if eval.safety == "crisis" {
                eprintln!("--> Vangrail geraakt: CRISIS_DETECTED");
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "CRISIS_DETECTED",
                )
                    .into_response();
            }
            if eval.safety == "pii" {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Bevat herleidbare persoonsgegevens (telefoon, mail of handle)",
                )
                    .into_response();
            }
            (eval.category, eval.valence.clamp(-1.0, 1.0), eval.safety)
        }
        Err(err) => {
            eprintln!("--> Fout bij moderatie-evaluatie: {:?}", err);
            (NoteCategory::Heavy, -0.5, "review".to_string())
        }
    };

    let initial_status = if safety == "safe" {
        "approved"
    } else {
        "pending"
    };

    let category_str = match category {
        NoteCategory::Light => "light",
        NoteCategory::Neutral => "neutral",
        NoteCategory::Heavy => "heavy",
    };

    let now = chrono::Utc::now();
    let truncated_timestamp = (now.timestamp() / 900) * 900;
    let truncated_time = chrono::DateTime::from_timestamp(truncated_timestamp, 0)
        .unwrap_or(now);

    let insert_result = sqlx::query_as::<_, Note>(
        r#"
        INSERT INTO notes (text, category, valence, safety, moderation_status, created_at) 
        VALUES ($1, $2::note_category, $3, $4, $5::moderation_status, $6)
        RETURNING id, text, category, valence, resonances, created_at
        "#,
    )
    .bind(text)
    .bind(category_str)
    .bind(valence)
    .bind(safety)
    .bind(initial_status)
    .bind(truncated_time)
    .fetch_one(&state.db)
    .await;

    match insert_result {
        Ok(created_note) => {
            if initial_status == "approved" {
                let public_dto = PublicNoteDto::from(created_note);
                let _ = state.feed_tx.send(public_dto);
            } else if initial_status == "pending" {
                let payload = created_note.id.to_string();
                let _ = sqlx::query("SELECT pg_notify('new_pending_vent', $1)")
                    .bind(payload)
                    .execute(&state.db)
                    .await;
            }
            (StatusCode::CREATED, "Geplaatst").into_response()
        }
        Err(err) => {
            eprintln!("--> Fout bij opslaan in database: {:?}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Fout bij opslaan").into_response()
        }
    }
}

async fn post_resonate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let res = sqlx::query("UPDATE notes SET resonances = resonances + 1 WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    match res {
        Ok(result) if result.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            eprintln!("Fout bij ophogen resonantie: {:?}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}