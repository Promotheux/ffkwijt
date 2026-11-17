mod db;
mod feed;
mod models;
mod moderator;
mod ratelimit;
mod sanitizer;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use models::{
    CreateReportRequest, CreateVentRequest, FeedQuery, Note, NoteCategory, PublicNoteDto,
    REPORT_REASONS,
};
use moderator::Moderator;
use sqlx::postgres::PgListener;
use ratelimit::RateLimiter;
use std::{convert::Infallible, net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: db::DbPool,
    moderator: Arc<Moderator>,
    feed_tx: broadcast::Sender<PublicNoteDto>,
    report_limiter: Arc<RateLimiter>,
    post_limiter: Arc<RateLimiter>,
    report_hide_threshold: i64,
}

/// Maximaal aantal meldingen per IP-adres per uur (alleen in geheugen bijgehouden).
const REPORTS_PER_HOUR: usize = 5;
/// Maximaal aantal plaatsingen per IP-adres per 10 minuten. Ruim genoeg voor een
/// gedeeld campus-IP, maar stopt het volspammen van het prikbord.
const POSTS_PER_10_MIN: usize = 5;
/// Maximale lengte van de optionele toelichting bij een melding.
const REPORT_DETAILS_MAX: usize = 500;
/// Hoe lang meldingen bewaard blijven.
const REPORT_RETENTION_DAYS: i32 = 180;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL ontbreekt");
    let pool = db::init_db(&db_url).await?;

    let moderator = Arc::new(Moderator::new_embedded("models")?);

    // Channel buffer voor live feed updates naar de browsers (beperkt tot 64 slots)
    let (feed_tx, _feed_rx) = broadcast::channel::<PublicNoteDto>(feed::FEED_BROADCAST_CAPACITY);

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
                    SELECT id, text, category, valence, resonances, created_at
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

    let report_limiter = Arc::new(RateLimiter::new(
        REPORTS_PER_HOUR,
        Duration::from_secs(60 * 60),
    ));
    let post_limiter = Arc::new(RateLimiter::new(
        POSTS_PER_10_MIN,
        Duration::from_secs(10 * 60),
    ));

    // Optioneel: verberg een bericht automatisch (terug naar 'pending') zodra het
    // zoveel open meldingen heeft. 0 = uit; dan beslist het beheer altijd zelf.
    let report_hide_threshold: i64 = std::env::var("REPORT_HIDE_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // Periodiek opruimen: oude meldingen en verlopen rate-limit sleutels
    let cleanup_pool = pool.clone();
    let cleanup_limiter = report_limiter.clone();
    let cleanup_post_limiter = post_limiter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            cleanup_limiter.prune();
            cleanup_post_limiter.prune();
            if let Err(e) = sqlx::query(
                "DELETE FROM reports WHERE created_at < NOW() - make_interval(days => $1)",
            )
            .bind(REPORT_RETENTION_DAYS)
            .execute(&cleanup_pool)
            .await
            {
                eprintln!("Fout bij opruimen oude meldingen: {:?}", e);
            }
        }
    });

    let state = AppState {
        db: pool,
        moderator,
        feed_tx,
        report_limiter,
        post_limiter,
        report_hide_threshold,
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
        .route("/api/report/:id", post(post_report))
        .route_service("/", ServeFile::new("public/index.html"))
        .fallback_service(serve_dir)
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("ffkwijt backend met PostgreSQL luistert op http://127.0.0.1:3000");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

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
        SELECT id, text, category, valence, resonances, created_at 
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
        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
            tracing::warn!(skipped, "SSE-client loopt achter; stream blijft verbonden");
            None
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn post_vent(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
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

    // Tekens tellen, geen bytes: é, ë en emoji zijn meerdere bytes in UTF-8
    if text.is_empty() || text.chars().count() > 280 {
        return (StatusCode::BAD_REQUEST, "Ongeldige tekstlengte").into_response();
    }

    // Vóór de AI-check, zodat een flood ook geen rekenkracht kost
    if !state.post_limiter.check(&client_key(&headers, addr)) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Je hebt net al een paar gedachten geplaatst. Neem even rust en probeer het over een paar minuten opnieuw.",
        )
            .into_response();
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

    let (category, valence, safety) = match state.moderator.evaluate(text).await {
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

/// Bepaalt een sleutel voor de rate limiter. Achter Caddy komt het echte adres uit
/// X-Forwarded-For; zonder proxy valt het terug op het socketadres.
/// Deze waarde wordt nergens opgeslagen.
fn client_key(headers: &HeaderMap, addr: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| addr.ip().to_string())
}

async fn post_report(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreateReportRequest>,
) -> impl IntoResponse {
    // Honeypot: doe alsof het gelukt is
    if payload.website.as_deref().is_some_and(|w| !w.trim().is_empty()) {
        return (StatusCode::ACCEPTED, "Ontvangen").into_response();
    }

    if !REPORT_REASONS.contains(&payload.reason.as_str()) {
        return (StatusCode::BAD_REQUEST, "Ongeldige reden").into_response();
    }

    let details = payload
        .details
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| d.chars().take(REPORT_DETAILS_MAX).collect::<String>());

    if !state.report_limiter.check(&client_key(&headers, addr)) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Je hebt het afgelopen uur al veel gemeld. Probeer het later opnieuw.",
        )
            .into_response();
    }

    let now = chrono::Utc::now();
    let truncated_time =
        chrono::DateTime::from_timestamp((now.timestamp() / 900) * 900, 0).unwrap_or(now);

    // Alleen goedgekeurde, zichtbare berichten kunnen gemeld worden
    let insert = sqlx::query(
        r#"
        INSERT INTO reports (note_id, reason, details, created_at)
        SELECT id, $2, $3, $4 FROM notes
        WHERE id = $1 AND moderation_status = 'approved'
        "#,
    )
    .bind(id)
    .bind(&payload.reason)
    .bind(details)
    .bind(truncated_time)
    .execute(&state.db)
    .await;

    match insert {
        Ok(r) if r.rows_affected() == 0 => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Ok(_) => {}
        Err(err) => {
            eprintln!("Fout bij opslaan melding: {:?}", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Fout bij opslaan").into_response();
        }
    }

    // Seintje voor ffkwijt-admin
    let _ = sqlx::query("SELECT pg_notify('new_report', $1)")
        .bind(id.to_string())
        .execute(&state.db)
        .await;

    if state.report_hide_threshold > 0 {
        let hidden = sqlx::query(
            r#"
            UPDATE notes SET moderation_status = 'pending'
            WHERE id = $1 AND moderation_status = 'approved'
            AND (SELECT COUNT(*) FROM reports WHERE note_id = $1 AND handled_at IS NULL) >= $2
            "#,
        )
        .bind(id)
        .bind(state.report_hide_threshold)
        .execute(&state.db)
        .await;

        if let Ok(r) = hidden {
            if r.rows_affected() > 0 {
                let _ = sqlx::query("SELECT pg_notify('new_pending_vent', $1)")
                    .bind(id.to_string())
                    .execute(&state.db)
                    .await;
            }
        }
    }

    (StatusCode::ACCEPTED, "Ontvangen").into_response()
}
