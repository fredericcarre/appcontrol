//! Lightweight Retrieval-Augmented Generation over local markdown
//! corpora.
//!
//! Configurable via the `RAG_CORPUS_DIR` environment variable. The
//! module recursively scans the directory at first call, chunks each
//! `.md` file into ~800-character windows, builds a simple inverted
//! token index, and answers queries with the top-k chunks ranked by
//! BM25-lite scoring (token frequency × IDF).
//!
//! Why not embeddings? Embedding-based RAG requires a model server
//! and pay-per-call costs. For runbook lookup over hundreds of
//! markdown pages, keyword retrieval is fast, deterministic, audit-
//! friendly, and ships with zero infra. When real semantic retrieval
//! is desired, swap `RagIndex::search` for an embedding lookup.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunk {
    pub source: String,        // relative file path
    pub chunk_index: usize,
    pub text: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagAnswer {
    pub query: String,
    pub corpus_dir: String,
    pub chunk_count: usize,
    pub matches: Vec<RagChunk>,
}

struct InvertedIndex {
    /// All chunks, in insertion order.
    chunks: Vec<(String, usize, String)>, // (source, chunk_index, text)
    /// Per-token postings: token -> Vec<(chunk_idx, term_freq)>
    postings: HashMap<String, Vec<(usize, usize)>>,
    /// Document frequency per token (for IDF).
    doc_freq: HashMap<String, usize>,
    /// Total document count.
    total: usize,
}

impl InvertedIndex {
    fn new() -> Self {
        Self {
            chunks: Vec::new(),
            postings: HashMap::new(),
            doc_freq: HashMap::new(),
            total: 0,
        }
    }

    fn add(&mut self, source: String, chunk_index: usize, text: String) {
        let chunk_id = self.chunks.len();
        let tokens = tokenize(&text);
        let mut freq: HashMap<&str, usize> = HashMap::new();
        for t in &tokens {
            *freq.entry(t.as_str()).or_insert(0) += 1;
        }
        for (tok, count) in freq {
            self.postings
                .entry(tok.to_string())
                .or_default()
                .push((chunk_id, count));
            *self.doc_freq.entry(tok.to_string()).or_insert(0) += 1;
        }
        self.chunks.push((source, chunk_index, text));
        self.total += 1;
    }

    fn search(&self, query: &str, k: usize) -> Vec<RagChunk> {
        if self.total == 0 {
            return Vec::new();
        }
        let q_tokens = tokenize(query);
        let mut scores: HashMap<usize, f64> = HashMap::new();
        for tok in q_tokens {
            if let Some(postings) = self.postings.get(&tok) {
                let df = *self.doc_freq.get(&tok).unwrap_or(&1) as f64;
                let idf = ((self.total as f64) / df).ln().max(0.0);
                for (chunk_id, tf) in postings {
                    *scores.entry(*chunk_id).or_insert(0.0) += (*tf as f64) * idf;
                }
            }
        }
        let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(k);
        ranked
            .into_iter()
            .map(|(chunk_id, score)| {
                let (source, idx, text) = self.chunks[chunk_id].clone();
                RagChunk {
                    source,
                    chunk_index: idx,
                    text,
                    score,
                }
            })
            .collect()
    }
}

// Tokens are lower-cased alphanumeric sequences ≥ 3 chars; stopwords
// from a tiny list. Good enough for runbook keyword retrieval.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| !STOPWORDS.contains(&s.as_str()))
        .collect()
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "are", "but", "not",
    "you", "all", "can", "has", "had", "have", "was", "were", "les", "des",
    "une", "pour", "que", "qui", "est", "dans", "par", "aux", "avec", "sur",
    "sont", "été", "ses", "son", "sa", "ces", "cette", "leur",
];

fn chunk_markdown(content: &str, target_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in content.split("\n\n") {
        if current.len() + paragraph.len() > target_chars && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn walk_markdown(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_markdown(&path, base, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
                out.push((rel, content));
            }
        }
    }
}

struct CorpusState {
    dir: PathBuf,
    index: InvertedIndex,
}

static INDEX: LazyLock<RwLock<Option<CorpusState>>> = LazyLock::new(|| RwLock::new(None));

fn ensure_loaded(dir: &Path) {
    {
        let guard = INDEX.read().unwrap();
        if let Some(state) = &*guard {
            if state.dir == dir {
                return;
            }
        }
    }
    let mut files = Vec::new();
    walk_markdown(dir, dir, &mut files);
    let mut idx = InvertedIndex::new();
    for (rel, content) in files {
        for (i, chunk) in chunk_markdown(&content, 800).into_iter().enumerate() {
            idx.add(rel.to_string_lossy().to_string(), i, chunk);
        }
    }
    let mut guard = INDEX.write().unwrap();
    *guard = Some(CorpusState {
        dir: dir.to_path_buf(),
        index: idx,
    });
}

/// Query the runbook RAG. Returns `None` when `RAG_CORPUS_DIR` is unset.
pub fn query(question: &str, top_k: usize) -> Option<RagAnswer> {
    let dir = std::env::var("RAG_CORPUS_DIR").ok().map(PathBuf::from)?;
    if !dir.exists() {
        return Some(RagAnswer {
            query: question.to_string(),
            corpus_dir: dir.to_string_lossy().to_string(),
            chunk_count: 0,
            matches: Vec::new(),
        });
    }
    ensure_loaded(&dir);
    let guard = INDEX.read().unwrap();
    let state = guard.as_ref()?;
    let matches = state.index.search(question, top_k);
    Some(RagAnswer {
        query: question.to_string(),
        corpus_dir: state.dir.to_string_lossy().to_string(),
        chunk_count: state.index.total,
        matches,
    })
}

/// Force a re-index (e.g. after a runbook update).
pub fn reload() {
    let mut guard = INDEX.write().unwrap();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_strips_short_words_and_stopwords() {
        let toks = tokenize("The quick brown fox jumps over a lazy dog");
        assert!(toks.contains(&"quick".to_string()));
        assert!(toks.contains(&"brown".to_string()));
        assert!(toks.contains(&"lazy".to_string()));
        assert!(!toks.contains(&"the".to_string()));
        assert!(!toks.iter().any(|t| t.len() < 3));
    }

    #[test]
    fn chunk_markdown_respects_target_size() {
        let text = (0..10).map(|i| format!("para{}", i)).collect::<Vec<_>>().join("\n\n");
        let chunks = chunk_markdown(&text, 20);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(!c.is_empty());
        }
    }

    #[test]
    fn empty_index_search_returns_empty() {
        let idx = InvertedIndex::new();
        assert!(idx.search("anything", 5).is_empty());
    }

    #[test]
    fn search_ranks_by_token_frequency_and_idf() {
        let mut idx = InvertedIndex::new();
        idx.add("a.md".into(), 0, "kafka cluster broker partition".into());
        idx.add("b.md".into(), 0, "postgres database backup procedure".into());
        idx.add("c.md".into(), 0, "kafka topic kafka offset kafka log".into());
        let results = idx.search("kafka", 5);
        assert!(!results.is_empty());
        // c.md has highest token frequency for "kafka" → top.
        assert_eq!(results[0].source, "c.md");
    }
}
