# ffkwijt — API & Inference Engine

High-performance, async backend service voor het **ffkwijt** platform. Gebouwd met Rust (Axum) en aangedreven door een lokaal gefinetuned, gekwantiseerd ONNX-model voor real-time sentiment- en moderatieclassificatie.

---

## Architectuur Overzicht

```
[ Client / Web ]
       │
       ▼
[ Caddy Reverse Proxy ]
       │
       ├─► / (Static Files, zstd, clean URLs)
       └─► /api/* ───────────────┐
                                 ▼
                     [ Axum HTTP Engine (:3000) ]
                                 │
                 ┌───────────────┴───────────────┐
                 ▼                               ▼
       [ ONNX Runtime Engine ]          [ PostgreSQL Pool ]
       • distilbert-base-multilingual   • sqlx connection pool
       • Sentiment / Valence            • Moderated feeds
       • Categorisatie (3 klassen)
```

- **Runtime & Web Framework:** [Axum](https://github.com/tokio-rs/axum) op [Tokio](https://tokio.rs/)
- **Database:** PostgreSQL via [SQLx](https://github.com/launchbadge/sqlx) (connection pooled, compile-time query checks)
- **Inference:** In-memory [ONNX Runtime](https://onnxruntime.ai/) (`ort` crate) met HuggingFace `tokenizers`
- **Real-time updates:** Server-Sent Events (SSE) via `tokio::sync::broadcast`

---

## Projectstructuur

```
.
├── src/
│   ├── main.rs         # Server setup, routing, state & graceful shutdown
│   ├── db.rs           # PostgreSQL pool configuratie en query helpers
│   ├── feed.rs         # SSE broadcasting & feed streaming endpoints
│   ├── models.rs       # Serde structs, request/response DTO's
│   ├── moderator.rs    # In-memory ONNX tokenizer en inferentie logica
│   └── sanitizer.rs    # Input sanitization en HTML escaping
├── models/             # Tokenizer config & ONNX model artefacten (*.onnx genegeerd in git)
├── train.py            # Local fine-tuning & ONNX export pipeline
├── Cargo.toml          # Rust dependencies & build profielen
└── Dockerfile          # Multi-stage production container build
```

---

## Lokale Ontwikkeling

### Vereisten
- **Rust:** `1.80+` (stable)
- **PostgreSQL:** `15+