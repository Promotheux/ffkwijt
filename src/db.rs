use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::time::Duration;

pub type DbPool = Pool<Postgres>;

pub async fn init_db(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(database_url)
        .await?;

    // 1. note_category enum
    sqlx::query(
        r#"
        DO $$ BEGIN
            CREATE TYPE note_category AS ENUM ('light', 'neutral', 'heavy');
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
        "#,
    )
    .execute(&pool)
    .await?;

    // 2. moderation_status enum
    sqlx::query(
        r#"
        DO $$ BEGIN
            CREATE TYPE moderation_status AS ENUM ('pending', 'approved', 'rejected');
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
        "#,
    )
    .execute(&pool)
    .await?;

    // 3. notes tabel
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS notes (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            text TEXT NOT NULL,
            category note_category NOT NULL,
            valence REAL NOT NULL,
            safety VARCHAR(32) NOT NULL DEFAULT 'safe',
            moderation_status moderation_status NOT NULL DEFAULT 'approved',
            resonances INT NOT NULL DEFAULT 0,
            corrected_category note_category,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            reviewed_at TIMESTAMPTZ
        );
        "#,
    )
    .execute(&pool)
    .await?;

    // 4. composite index
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_notes_moderation_created_at
        ON notes (moderation_status, created_at DESC);
        "#,
    )
    .execute(&pool)
    .await?;

    seed_if_empty(&pool).await?;

    Ok(pool)
}

async fn seed_if_empty(pool: &DbPool) -> Result<(), sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notes")
        .fetch_one(pool)
        .await?;

    if count.0 == 0 {
        let seeds = vec![
            (
                "Ik doe alsof alles vanzelf gaat, maar ik ben zo intens moe van mezelf en alle verwachtingen.",
                "heavy",
                -0.8,
            ),
            (
                "Soms wil ik gewoon een week verdwijnen zonder dat iemand vragen stelt of ongerust wordt.",
                "heavy",
                -0.7,
            ),
            (
                "Na drie weken uitstellen heb ik vanmiddag eindelijk die ene moeilijke mail verstuurd. Wat een opluchting.",
                "light",
                0.7,
            ),
            (
                "Gek hoe stil de straat kan zijn om 3 uur 's nachts. Alsof de hele wereld even op pauze staat.",
                "neutral",
                0.0,
            ),
            (
                "Ik vind het doodeng om nee te zeggen, dus zeg ik overal ja op tot ik instort.",
                "heavy",
                -0.75,
            ),
            (
                "Soms voelt een gewone dag al alsof ik een marathon moet lopen met lood in m'n schoenen.",
                "heavy",
                -0.7,
            ),
            (
                "Het voelt gek genoeg zo rustgevend om hier te lezen dat ik niet de enige ben die maar wat probeert.",
                "light",
                0.6,
            ),
            (
                "We doen allemaal maar alsof we volwassen zijn, maar volgens mij improviseert iedereen gewoon ter plekke.",
                "neutral",
                0.1,
            ),
        ];

        for (text, cat, val) in seeds {
            let now = chrono::Utc::now();

            sqlx::query(
                r#"
                INSERT INTO notes (text, category, valence, safety, moderation_status, created_at)
                VALUES ($1, $2::note_category, $3, 'safe', 'approved'::moderation_status, $4)
                "#,
            )
            .bind(text)
            .bind(cat)
            .bind(val)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}