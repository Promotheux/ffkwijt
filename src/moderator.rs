use crate::models::NoteCategory;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

pub struct ModerationEvaluation {
    pub category: NoteCategory,
    pub valence: f32,
    pub safety: String,
}

#[derive(Clone)]
pub struct Moderator {
    tokenizer: Arc<Tokenizer>,
    session: Arc<Mutex<Session>>,
}

impl Moderator {
    pub fn new_embedded(model_dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = model_dir.as_ref();

        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| format!("Kon tokenizer.json niet laden: {e}"))?;

        // Intra-threads expliciet op 1 instellen om thread-contention/oversubscription
        // in een multithreaded webserver met tokio blocking threads te voorkomen.
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?
            .commit_from_file(dir.join("model.onnx"))?;

        Ok(Self {
            tokenizer: Arc::new(tokenizer),
            session: Arc::new(Mutex::new(session)),
        })
    }

    /// Asynchrone evaluatie die tokenization en inferentie offload naar Tokio's blocking threadpool.
    pub async fn evaluate(&self, text: &str) -> Result<ModerationEvaluation, Box<dyn std::error::Error + Send + Sync>> {
        let moderator = self.clone();
        let text = text.to_string();

        tokio::task::spawn_blocking(move || moderator.evaluate_sync(&text))
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
    }

    fn evaluate_sync(&self, text: &str) -> Result<ModerationEvaluation, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Tokenize (max 128 tokens)
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenization mislukt: {e}"))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();
        let seq_len = input_ids.len();

        let input_ids_val = Value::from_array(([1usize, seq_len], input_ids))?;
        let attention_mask_val = Value::from_array(([1usize, seq_len], attention_mask))?;

        // 2. Lock de sessie voor inference
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Sessie mutex vergrendeling mislukt")?;

        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_val,
            "attention_mask" => attention_mask_val,
        ])?;

        // 3. Logits uitlezen
        let extracted = outputs[0].try_extract_tensor::<f32>()?;
        let logits_slice: &[f32] = extracted.1;

        // 4. Softmax probabilities berekenen
        let max_l = logits_slice.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = logits_slice.iter().map(|&x| (x - max_l).exp()).sum();
        let probs: Vec<f32> = logits_slice
            .iter()
            .map(|&x| (x - max_l).exp() / exp_sum)
            .collect();

        let prob_light = probs.get(0).copied().unwrap_or(0.0);
        let prob_neutral = probs.get(1).copied().unwrap_or(0.0);
        let prob_heavy = probs.get(2).copied().unwrap_or(0.0);

        // Debug logging van ruwe logits en berekende kansen
        eprintln!(
            "--> Logits: {:?} | Probs: [Light: {:.2}, Neutral: {:.2}, Heavy: {:.2}]",
            logits_slice, prob_light, prob_neutral, prob_heavy
        );

        let valence = (prob_light * 0.7) - (prob_heavy * 0.7);

        let category = if prob_heavy >= prob_neutral && prob_heavy >= prob_light {
            NoteCategory::Heavy
        } else if prob_light >= prob_neutral && prob_light >= prob_heavy {
            NoteCategory::Light
        } else {
            NoteCategory::Neutral
        };

        let safety = if prob_heavy > 0.88 {
            "review".to_string()
        } else {
            "safe".to_string()
        };

        Ok(ModerationEvaluation {
            category,
            valence: valence.clamp(-1.0, 1.0),
            safety,
        })
    }
}