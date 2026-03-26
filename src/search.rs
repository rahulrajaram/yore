use ahash::AHasher;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::types::*;
use crate::util::default_query_stop_words;

// BM25 tuning constants
pub const BM25_K1: f64 = 1.5;
pub const BM25_B: f64 = 0.75;

pub fn extract_keywords(text: &str) -> Vec<String> {
    extract_keywords_with_options(text, true)
}

pub fn extract_keywords_with_options(text: &str, filter_stopwords: bool) -> Vec<String> {
    let stop_words: HashSet<&str> = default_query_stop_words().iter().copied().collect();

    let word_re = Regex::new(r"[a-zA-Z][a-zA-Z0-9_-]*").unwrap();

    word_re
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|w| w.len() >= 3 && (!filter_stopwords || !stop_words.contains(w.as_str())))
        .collect()
}

pub fn parse_query_terms(query: &str, filter_stopwords: bool) -> Vec<String> {
    extract_keywords_with_options(query, filter_stopwords)
}

pub fn parse_query(query: &str, filter_stopwords: bool) -> ParsedQuery {
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut buffer = String::new();
    let mut in_quote = false;

    for ch in query.chars() {
        if ch == '"' {
            let trimmed = buffer.trim();
            if !trimmed.is_empty() {
                parts.push((trimmed.to_string(), in_quote));
            }
            buffer.clear();
            in_quote = !in_quote;
            continue;
        }
        buffer.push(ch);
    }

    let trimmed = buffer.trim();
    if !trimmed.is_empty() {
        parts.push((trimmed.to_string(), in_quote));
    }
    let mut terms = Vec::new();
    let mut phrases = Vec::new();

    for (text, is_phrase) in parts {
        let parsed_terms = parse_query_terms(&text, filter_stopwords);
        terms.extend(parsed_terms.iter().cloned());
        if is_phrase {
            let phrase_terms = extract_keywords_with_options(&text, false);
            if !phrase_terms.is_empty() {
                phrases.push(PhraseGroup {
                    terms: phrase_terms,
                });
            }
        }
    }

    ParsedQuery { terms, phrases }
}

/// Simple suffix-stripping stemmer
pub fn stem_word(word: &str) -> String {
    let w = word.to_lowercase();

    // Common suffixes to strip
    let suffixes = [
        "ization", "ational", "iveness", "fulness", "ousness", "ation", "ement", "ment", "able",
        "ible", "ness", "ical", "ings", "ing", "ies", "ive", "ful", "ous", "ity", "ed", "ly", "er",
        "es", "s",
    ];

    for suffix in suffixes {
        if w.len() > suffix.len() + 2 && w.ends_with(suffix) {
            return w[..w.len() - suffix.len()].to_string();
        }
    }

    w
}

/// Extract top N distinctive terms from a document, excluding query terms.
/// Returns human-readable (unstemmed) terms ranked by TF-IDF.
pub fn get_top_doc_terms(
    entry: &FileEntry,
    idf_map: &HashMap<String, f64>,
    exclude_terms: &[String],
    n: usize,
) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }

    // Stem the exclusion terms for comparison
    let exclude_stemmed: HashSet<String> = exclude_terms
        .iter()
        .map(|t| stem_word(&t.to_lowercase()))
        .collect();

    // Collect unique keywords with their TF-IDF scores
    // Use body_keywords (unstemmed) but rank by term_frequencies (stemmed)
    let mut seen_stems: HashSet<String> = HashSet::new();
    let mut term_scores: Vec<(String, f64)> = Vec::new();

    for kw in entry.body_keywords.iter().chain(entry.keywords.iter()) {
        let stemmed = stem_word(&kw.to_lowercase());

        // Skip if already seen this stem, or if it's an excluded term
        if seen_stems.contains(&stemmed) || exclude_stemmed.contains(&stemmed) {
            continue;
        }
        seen_stems.insert(stemmed.clone());

        // Calculate TF-IDF score
        let tf = *entry.term_frequencies.get(&stemmed).unwrap_or(&0) as f64;
        let idf = *idf_map.get(&stemmed).unwrap_or(&0.0);
        let score = tf * idf;

        if score > 0.0 {
            term_scores.push((kw.to_lowercase(), score));
        }
    }

    // Sort by score descending
    term_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top N
    term_scores
        .into_iter()
        .take(n)
        .map(|(term, _)| term)
        .collect()
}

/// Compute simhash fingerprint for content
pub fn compute_simhash(content: &str) -> u64 {
    let mut v = [0i32; 64];

    // Extract features (word shingles)
    let words: Vec<&str> = content.split_whitespace().collect();

    for window in words.windows(3) {
        let shingle = format!("{} {} {}", window[0], window[1], window[2]);
        let h = hash_string(&shingle);

        for (i, item) in v.iter_mut().enumerate() {
            if (h >> i) & 1 == 1 {
                *item += 1;
            } else {
                *item -= 1;
            }
        }
    }

    // Convert to fingerprint
    let mut fingerprint: u64 = 0;
    for (i, item) in v.iter().enumerate() {
        if *item > 0 {
            fingerprint |= 1 << i;
        }
    }

    fingerprint
}

pub fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Count differing bits between two simhashes (Hamming distance)
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Convert hamming distance to similarity (0.0 to 1.0)
pub fn simhash_similarity(a: u64, b: u64) -> f64 {
    let distance = hamming_distance(a, b);
    1.0 - (f64::from(distance) / 64.0)
}

/// Index sections of a document with SimHash fingerprints
pub fn index_sections(content: &str, headings: &[Heading]) -> Vec<SectionFingerprint> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();

    if headings.is_empty() {
        return sections;
    }

    for i in 0..headings.len() {
        let start = headings[i].line.saturating_sub(1);
        let end = headings
            .get(i + 1)
            .map_or(lines.len(), |h| h.line.saturating_sub(1));

        // Extract section text
        let section_text = lines[start..end].join("\n");

        sections.push(SectionFingerprint {
            heading: headings[i].text.clone(),
            level: headings[i].level,
            line_start: start + 1,
            line_end: end,
            simhash: compute_simhash(&section_text),
        });
    }

    sections
}

/// Compute MinHash signature for a set of keywords
pub fn compute_minhash(keywords: &[String], num_hashes: usize) -> Vec<u64> {
    let mut hashes = vec![u64::MAX; num_hashes];

    for keyword in keywords {
        for (i, hash_slot) in hashes.iter_mut().enumerate().take(num_hashes) {
            let mut hasher = AHasher::default();
            keyword.hash(&mut hasher);
            i.hash(&mut hasher); // Use index as seed
            let h = hasher.finish();

            *hash_slot = (*hash_slot).min(h);
        }
    }

    hashes
}

/// Compute MinHash similarity (Jaccard estimate)
pub fn minhash_similarity(a: &[u64], b: &[u64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();

    matches as f64 / a.len() as f64
}

/// Compute BM25 score for a document given query terms
pub fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    if doc.doc_length == 0 {
        return 0.0;
    }

    let mut score = 0.0;
    let norm_factor = 1.0 - BM25_B + BM25_B * (doc.doc_length as f64 / avg_doc_length);

    for term in query_terms {
        let stemmed = stem_word(&term.to_lowercase());
        let tf = *doc.term_frequencies.get(&stemmed).unwrap_or(&0) as f64;
        let idf = idf_map.get(&stemmed).unwrap_or(&0.0);

        if tf > 0.0 {
            score += idf * (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * norm_factor);
        }
    }

    score
}

/// Build LSH buckets for fast duplicate detection
pub fn lsh_buckets(files: &HashMap<String, FileEntry>, bands: usize) -> HashMap<u64, Vec<String>> {
    let rows_per_band = 128 / bands; // Assuming 128 hashes
    let mut buckets: HashMap<u64, Vec<String>> = HashMap::new();

    for (path, entry) in files {
        if entry.minhash.is_empty() {
            continue; // Skip files without MinHash
        }

        for band in 0..bands {
            let start = band * rows_per_band;
            let end = (start + rows_per_band).min(entry.minhash.len());

            // Hash this band's values
            let mut hasher = AHasher::default();
            for val in &entry.minhash[start..end] {
                val.hash(&mut hasher);
            }
            let band_hash = hasher.finish();

            buckets.entry(band_hash).or_default().push(path.clone());
        }
    }

    buckets
}

pub fn contains_phrase_tokens(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
