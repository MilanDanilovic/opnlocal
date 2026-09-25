//! Finding the parts of a long document that matter for a question, when the whole document
//! doesn't fit the model's context. Plain word matching (BM25) over fixed-size chunks: no extra
//! model to download, instant, and it works for any language written in words. Chunks are
//! returned in document order, so the model reads them as a shortened document.

use std::collections::HashMap;

/// Words per chunk. Around 200 tokens of prose: enough for a fact and its surroundings.
const CHUNK_WORDS: usize = 150;
/// BM25 constants (term-frequency saturation, length normalisation).
const K1: f32 = 1.2;
const B: f32 = 0.75;

/// A piece of a document; `index` is its position in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub index: usize,
    pub text: String,
}

/// Splits on whitespace into chunks of [`CHUNK_WORDS`] words, breaking at a sentence end where
/// one is near.
pub fn chunks(text: &str) -> Vec<Chunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let mut end = (start + CHUNK_WORDS).min(words.len());
        if end < words.len() {
            // Prefer to end after a sentence, looking back up to a third of the chunk.
            if let Some(cut) = (start + CHUNK_WORDS * 2 / 3..end).rev().find(|&i| words[i].ends_with(['.', '!', '?'])) {
                end = cut + 1;
            }
        }
        out.push(Chunk { index: out.len(), text: words[start..end].join(" ") });
        start = end;
    }
    out
}

/// Lowercased alphanumeric words of at least two characters.
fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(|w| w.to_lowercase())
        .collect()
}

/// BM25 score of every chunk for `query`, in chunk order. All zero when no query word occurs.
pub fn scores(query: &str, chunks: &[Chunk]) -> Vec<f32> {
    let docs: Vec<Vec<String>> = chunks.iter().map(|c| terms(&c.text)).collect();
    let n = docs.len() as f32;
    let avg_len = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n.max(1.0);
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        let mut seen: Vec<&str> = Vec::new();
        for t in d {
            if !seen.contains(&t.as_str()) {
                seen.push(t);
                *df.entry(t).or_default() += 1;
            }
        }
    }
    let query_terms = terms(query);
    docs.iter()
        .map(|d| {
            let len = d.len() as f32;
            query_terms
                .iter()
                .map(|q| {
                    let tf = d.iter().filter(|t| *t == q).count() as f32;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let dfq = df.get(q.as_str()).copied().unwrap_or(0) as f32;
                    let idf = ((n - dfq + 0.5) / (dfq + 0.5) + 1.0).ln();
                    idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avg_len.max(1.0)))
                })
                .sum()
        })
        .collect()
}

/// The chunks most relevant to `query` that fit `budget_tokens` (see `chat::estimate_tokens`),
/// joined in document order with a gap mark between non-adjacent ones. With no matching words
/// (a question like "summarise this"), the document's beginning is used instead.
pub fn select(query: &str, text: &str, budget_tokens: u32) -> String {
    let all = chunks(text);
    let scores = scores(query, &all);
    let mut order: Vec<usize> = (0..all.len()).collect();
    if scores.iter().any(|&s| s > 0.0) {
        order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    }
    let mut chosen = Vec::new();
    let mut used = 0;
    for i in order {
        let cost = crate::chat::estimate_tokens(&all[i].text) + 2;
        if used + cost > budget_tokens {
            continue;
        }
        used += cost;
        chosen.push(i);
    }
    chosen.sort_unstable();
    let mut out = String::new();
    let mut last: Option<usize> = None;
    for i in chosen {
        if last.is_some_and(|l| l + 1 != i) {
            out.push_str("\n[…]\n");
        } else if last.is_some() {
            out.push(' ');
        }
        out.push_str(&all[i].text);
        last = Some(i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> String {
        let filler = |i: usize| format!("Paragraph {i} talks about the weather, the office plants and the coffee machine. ").repeat(12);
        let mut d = String::new();
        for i in 0..8 {
            if i == 5 {
                d.push_str("The report deadline is 14 October and the budget review follows a week later. ");
            }
            d.push_str(&filler(i));
        }
        d
    }

    #[test]
    fn chunks_cover_the_whole_text_in_order() {
        let d = document();
        let cs = chunks(&d);
        assert!(cs.len() > 5);
        assert_eq!(cs.iter().map(|c| c.index).collect::<Vec<_>>(), (0..cs.len()).collect::<Vec<_>>());
        let joined: String = cs.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined.split_whitespace().count(), d.split_whitespace().count());
    }

    #[test]
    fn the_relevant_chunk_wins_within_the_budget() {
        let d = document();
        let picked = select("When is the report deadline?", &d, 250);
        assert!(picked.contains("14 October"), "{picked}");
        assert!(crate::chat::estimate_tokens(&picked) <= 250);
        assert!(picked.len() < d.len() / 3);
    }

    #[test]
    fn vague_questions_get_the_beginning() {
        let d = document();
        let picked = select("summarise this", &d, 300);
        assert!(picked.starts_with("Paragraph 0"), "{picked}");
    }

    #[test]
    fn selected_chunks_stay_in_document_order_with_gap_marks() {
        let d = document();
        let picked = select("deadline Paragraph 0 coffee", &d, 600);
        let a = picked.find("Paragraph 0").unwrap();
        let b = picked.find("14 October").unwrap();
        assert!(a < b);
        assert!(picked.contains("[…]"));
    }
}
