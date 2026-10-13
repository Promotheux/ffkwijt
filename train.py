import json
import shutil
from pathlib import Path
import numpy as np
from datasets import Dataset
from sklearn.metrics import accuracy_score, f1_score
import torch
from transformers import (
    AutoModelForSequenceClassification,
    AutoTokenizer,
    Trainer,
    TrainingArguments,
)
from optimum.onnxruntime import ORTModelForSequenceClassification

DATASET_FILES = [
    Path("distillation_dataset_twopass.jsonl"),
    Path("moderated_notes.jsonl"),
]
BASE_MODEL = "distilbert-base-multilingual-cased"
TEMP_PYTORCH_DIR = Path("./temp_finetuned_pytorch")
FINAL_MODELS_DIR = Path("./models")

# Mappings exact synchroon met Axum/Rust moderator:
LABEL2ID = {"light": 0, "neutral": 1, "heavy": 2}
ID2LABEL = {0: "light", 1: "neutral", 2: "heavy"}


def load_dataset_from_jsonl():
    texts, labels = [], []
    seen_texts = set()

    for file_path in DATASET_FILES:
        if not file_path.exists():
            print(f"[!] Overgeslagen (bestand niet gevonden): {file_path}")
            continue

        with open(file_path, "r", encoding="utf-8-sig") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                item = json.loads(line)

                # Ondersteunt "label" (uit export), "category" en geneste "ground_truth"
                cat = item.get("label") or item.get("category")
                if not cat and isinstance(item.get("ground_truth"), dict):
                    cat = item["ground_truth"].get("category")

                text = item.get("text", "").strip()

                if cat and cat.lower() in LABEL2ID and text:
                    if text not in seen_texts:
                        seen_texts.add(text)
                        texts.append(text)
                        labels.append(LABEL2ID[cat.lower()])

    print(f"--> Ingeladen: {len(texts)} unieke samples")
    if len(texts) == 0:
        raise ValueError("Geen geldige records gevonden in de opgegeven datasets.")

    dataset = Dataset.from_dict({"text": texts, "label": labels})
    return dataset.train_test_split(test_size=0.15, seed=42)


def compute_metrics(eval_pred):
    logits, labels = eval_pred
    preds = np.argmax(logits, axis=1)
    acc = accuracy_score(labels, preds)
    f1 = f1_score(labels, preds, average="weighted")
    return {"accuracy": acc, "f1": f1}


def main():
    if not any(f.exists() for f in DATASET_FILES):
        raise FileNotFoundError(f"Geen van de dataset-bestanden gevonden in {Path.cwd()}")

    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"--> Training draait op: {device.upper()}")

    # 1. Inladen en splitsen
    splits = load_dataset_from_jsonl()

    # 2. Tokenizer
    tokenizer = AutoTokenizer.from_pretrained(BASE_MODEL)

    def tokenize_function(batch):
        return tokenizer(batch["text"], truncation=True, max_length=128)

    tokenized_splits = splits.map(tokenize_function, batched=True)

    # 3. Model opzetten
    model = AutoModelForSequenceClassification.from_pretrained(
        BASE_MODEL,
        num_labels=3,
        id2label=ID2LABEL,
        label2id=LABEL2ID,
    )

    # 4. Training instellingen
    training_args = TrainingArguments(
        output_dir="./checkpoints",
        eval_strategy="epoch",
        save_strategy="epoch",
        learning_rate=2e-5,
        per_device_train_batch_size=16,
        per_device_eval_batch_size=16,
        num_train_epochs=8,
        weight_decay=0.01,
        fp16=torch.cuda.is_available(),
        logging_steps=20,
        save_total_limit=1,
        load_best_model_at_end=True,
        metric_for_best_model="f1",
        report_to="none",
    )

    trainer = Trainer(
        model=model,
        args=training_args,
        train_dataset=tokenized_splits["train"],
        eval_dataset=tokenized_splits["test"],
        processing_class=tokenizer,
        compute_metrics=compute_metrics,
    )

    print("--> Start fine-tuning...")
    trainer.train()

    # 5. Opslaan en converteren naar ONNX
    print(f"--> PyTorch model opslaan naar {TEMP_PYTORCH_DIR}...")
    model.save_pretrained(TEMP_PYTORCH_DIR)
    tokenizer.save_pretrained(TEMP_PYTORCH_DIR)
    tokenizer = AutoTokenizer.from_pretrained(
        TEMP_PYTORCH_DIR, fix_mistral_regex=False
    )

    print(f"--> Converteren naar geoptimaliseerde ONNX in {FINAL_MODELS_DIR}...")
    FINAL_MODELS_DIR.mkdir(parents=True, exist_ok=True)

    ort_model = ORTModelForSequenceClassification.from_pretrained(
        TEMP_PYTORCH_DIR, export=True
    )
    ort_model.save_pretrained(FINAL_MODELS_DIR)
    tokenizer.save_pretrained(FINAL_MODELS_DIR)

    # 6. Opruimen
    shutil.rmtree(TEMP_PYTORCH_DIR, ignore_errors=True)
    shutil.rmtree("./checkpoints", ignore_errors=True)
    print("--> Gereed! models/model.onnx en models/tokenizer.json zijn bijgewerkt.")


if __name__ == "__main__":
    main()