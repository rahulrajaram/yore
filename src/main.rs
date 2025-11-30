use ahash::AHasher;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// yore - Fast document indexer for finding duplicates and searching content
#[derive(Parser)]
#[command(name = "yore")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true, default_value = ".yore.toml")]
    config: PathBuf,

    /// Quiet mode - suppress non-essential output
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Build forward and reverse indexes
    Build {
        /// Path to index
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory for indexes
        #[arg(short, long, default_value = ".yore")]
        output: PathBuf,

        /// File extensions to index (comma-separated)
        #[arg(short, long, default_value = "md,txt,rst")]
        types: String,

        /// Patterns to exclude (can be repeated)
        #[arg(short, long)]
        exclude: Vec<String>,
    },

    /// Search the index for keywords
    Query {
        /// Search terms
        terms: Vec<String>,

        /// Maximum results to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Show only file paths
        #[arg(short = 'l', long)]
        files_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find documents similar to a reference file
    Similar {
        /// Reference file
        file: PathBuf,

        /// Maximum results to show
        #[arg(short = 'n', long, default_value = "5")]
        limit: usize,

        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.3")]
        threshold: f64,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find duplicate/overlapping documents
    Dupes {
        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.35")]
        threshold: f64,

        /// Group duplicates together
        #[arg(long)]
        group: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find duplicate sections across documents
    DupesSections {
        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.7")]
        threshold: f64,

        /// Minimum number of files sharing a section
        #[arg(short = 'n', long, default_value = "2")]
        min_files: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Show what's shared between two files
    Diff {
        /// First file
        file1: PathBuf,

        /// Second file
        file2: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Show index statistics
    Stats {
        /// Show top N keywords
        #[arg(long, default_value = "20")]
        top_keywords: usize,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Interactive query mode
    Repl {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Assemble context digest for LLM consumption
    Assemble {
        /// Natural language query/question
        query: Vec<String>,

        /// Maximum tokens in output (approximate)
        #[arg(short = 't', long, default_value = "8000")]
        max_tokens: usize,

        /// Maximum sections to include
        #[arg(short = 's', long, default_value = "20")]
        max_sections: usize,

        /// Cross-reference expansion depth
        #[arg(short = 'd', long, default_value = "1")]
        depth: usize,

        /// Output format
        #[arg(short = 'f', long, default_value = "markdown")]
        format: String,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },
}

// Index structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>,  // keywords from full text
    links: Vec<Link>,
    simhash: u64,  // content fingerprint
    #[serde(default)]
    term_frequencies: HashMap<String, usize>,  // term counts for BM25
    #[serde(default)]
    doc_length: usize,  // total terms for BM25
    #[serde(default)]
    minhash: Vec<u64>,  // MinHash signature for LSH
    #[serde(default)]
    section_fingerprints: Vec<SectionFingerprint>,  // NEW: section-level SimHash
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Heading {
    line: usize,
    level: usize,
    text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Link {
    line: usize,
    text: String,
    target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SectionFingerprint {
    heading: String,
    level: usize,
    line_start: usize,
    line_end: usize,
    simhash: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ReverseEntry {
    file: String,
    line: Option<usize>,
    heading: Option<String>,
    level: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ForwardIndex {
    files: HashMap<String, FileEntry>,
    indexed_at: String,
    version: u32,  // index version for compatibility
    #[serde(default)]
    avg_doc_length: f64,  // NEW: average document length for BM25
    #[serde(default)]
    idf_map: HashMap<String, f64>,  // NEW: IDF scores for BM25
}

#[derive(Serialize, Deserialize, Debug)]
struct ReverseIndex {
    keywords: HashMap<String, Vec<ReverseEntry>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct IndexStats {
    total_files: usize,
    total_keywords: usize,
    total_headings: usize,
    total_links: usize,
    indexed_at: String,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Build { path, output, types, exclude } => {
            cmd_build(&path, &output, &types, &exclude, cli.quiet)
        }
        Commands::Query { terms, limit, files_only, json, index } => {
            cmd_query(&terms, limit, files_only, json, &index)
        }
        Commands::Similar { file, limit, threshold, json, index } => {
            cmd_similar(&file, limit, threshold, json, &index)
        }
        Commands::Dupes { threshold, group, json, index } => {
            cmd_dupes(threshold, group, json, &index)
        }
        Commands::DupesSections { threshold, min_files, json, index } => {
            cmd_dupes_sections(threshold, min_files, json, &index)
        }
        Commands::Diff { file1, file2, index } => {
            cmd_diff(&file1, &file2, &index)
        }
        Commands::Stats { top_keywords, index } => {
            cmd_stats(top_keywords, &index)
        }
        Commands::Repl { index } => {
            cmd_repl(&index)
        }
        Commands::Assemble { query, max_tokens, max_sections, depth, format, index } => {
            cmd_assemble(&query.join(" "), max_tokens, max_sections, depth, &format, &index)
        }
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "error".red().bold(), e);
        std::process::exit(1);
    }
}

fn cmd_build(
    path: &Path,
    output: &Path,
    types: &str,
    exclude: &[String],
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    if !quiet {
        println!("{} {}", "Indexing".cyan().bold(), path.display());
    }

    // Parse file types
    let extensions: HashSet<String> = types.split(',').map(|s| s.trim().to_lowercase()).collect();

    // Build walker with ignore patterns
    let mut builder = WalkBuilder::new(path);
    builder.hidden(true).git_ignore(true).git_global(true);

    // Add custom excludes
    for pattern in exclude {
        builder.add_ignore(Path::new(pattern));
    }

    // Collect files
    let mut forward_index = ForwardIndex {
        files: HashMap::new(),
        indexed_at: chrono_now(),
        version: 3,  // Version 3 includes BM25 (term_frequencies, idf_map) and MinHash
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let mut reverse_index = ReverseIndex {
        keywords: HashMap::new(),
    };

    let mut file_count = 0;
    let mut total_headings = 0;
    let mut total_links = 0;

    for entry in builder.build().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Check extension
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !extensions.contains(&ext) {
            continue;
        }

        // Skip common non-content directories
        let path_str = path.to_string_lossy();
        if path_str.contains("node_modules")
            || path_str.contains(".git/")
            || path_str.contains("target/")
            || path_str.contains("vendor/")
            || path_str.contains("venv/")
            || path_str.contains("__pycache__")
        {
            continue;
        }

        // Index the file
        if let Ok(entry) = index_file(path) {
            let rel_path = path.strip_prefix(std::env::current_dir()?)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Update reverse index with heading keywords
            for keyword in &entry.keywords {
                let stemmed = stem_word(&keyword.to_lowercase());
                reverse_index.keywords
                    .entry(stemmed)
                    .or_insert_with(Vec::new)
                    .push(ReverseEntry {
                        file: rel_path.clone(),
                        line: None,
                        heading: None,
                        level: None,
                    });
            }

            // Update reverse index with body keywords
            for keyword in &entry.body_keywords {
                let stemmed = stem_word(&keyword.to_lowercase());
                reverse_index.keywords
                    .entry(stemmed)
                    .or_insert_with(Vec::new)
                    .push(ReverseEntry {
                        file: rel_path.clone(),
                        line: None,
                        heading: None,
                        level: None,
                    });
            }

            for heading in &entry.headings {
                let words = extract_keywords(&heading.text);
                for word in words {
                    let stemmed = stem_word(&word.to_lowercase());
                    reverse_index.keywords
                        .entry(stemmed)
                        .or_insert_with(Vec::new)
                        .push(ReverseEntry {
                            file: rel_path.clone(),
                            line: Some(heading.line),
                            heading: Some(heading.text.clone()),
                            level: Some(heading.level),
                        });
                }
            }

            total_headings += entry.headings.len();
            total_links += entry.links.len();
            file_count += 1;

            forward_index.files.insert(rel_path, entry);
        }
    }

    // Compute BM25 statistics (IDF and average document length)
    let total_docs = forward_index.files.len() as f64;
    let mut doc_frequencies: HashMap<String, usize> = HashMap::new();
    let mut total_length = 0;

    // Compute document frequencies
    for entry in forward_index.files.values() {
        total_length += entry.doc_length;
        for term in entry.term_frequencies.keys() {
            *doc_frequencies.entry(term.clone()).or_insert(0) += 1;
        }
    }

    // Compute IDF scores
    let mut idf_map: HashMap<String, f64> = HashMap::new();
    for (term, df) in doc_frequencies {
        let idf = ((total_docs - df as f64 + 0.5) / (df as f64 + 0.5)).ln();
        idf_map.insert(term, idf);
    }

    forward_index.avg_doc_length = if total_docs > 0.0 {
        total_length as f64 / total_docs
    } else {
        0.0
    };
    forward_index.idf_map = idf_map;

    // Create output directory
    fs::create_dir_all(output)?;

    // Write indexes
    let forward_path = output.join("forward_index.json");
    let reverse_path = output.join("reverse_index.json");
    let stats_path = output.join("stats.json");

    fs::write(&forward_path, serde_json::to_string_pretty(&forward_index)?)?;
    fs::write(&reverse_path, serde_json::to_string_pretty(&reverse_index)?)?;

    let stats = IndexStats {
        total_files: file_count,
        total_keywords: reverse_index.keywords.len(),
        total_headings,
        total_links,
        indexed_at: chrono_now(),
    };
    fs::write(&stats_path, serde_json::to_string_pretty(&stats)?)?;

    let elapsed = start.elapsed();

    if !quiet {
        println!();
        println!("{}", "Index Statistics".green().bold());
        println!("  Files indexed:    {}", file_count.to_string().cyan());
        println!("  Unique keywords:  {}", reverse_index.keywords.len().to_string().cyan());
        println!("  Total headings:   {}", total_headings.to_string().cyan());
        println!("  Total links:      {}", total_links.to_string().cyan());
        println!("  Time elapsed:     {:.2?}", elapsed);
        println!();
        println!("{} {}", "Indexes written to".green(), output.display().to_string().cyan());
    }

    Ok(())
}

fn index_file(path: &Path) -> Result<FileEntry, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let metadata = fs::metadata(path)?;

    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    // Extract headings (markdown)
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$")?;
    let mut headings = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = heading_re.captures(line) {
            headings.push(Heading {
                line: i + 1,
                level: caps.get(1).map(|m| m.as_str().len()).unwrap_or(1),
                text: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            });
        }
    }

    // Extract links
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)")?;
    let mut links = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        for caps in link_re.captures_iter(line) {
            links.push(Link {
                line: i + 1,
                text: caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                target: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            });
        }
    }

    // Extract keywords from headings
    let mut keywords: HashSet<String> = HashSet::new();
    for heading in &headings {
        for kw in extract_keywords(&heading.text) {
            keywords.insert(stem_word(&kw));
        }
    }

    // NEW: Extract keywords from full body text
    let mut body_keywords: HashSet<String> = HashSet::new();
    for line in &lines {
        // Skip code blocks
        if line.starts_with("```") || line.starts_with("    ") {
            continue;
        }
        for kw in extract_keywords(line) {
            body_keywords.insert(stem_word(&kw));
        }
    }
    // Remove heading keywords from body to avoid duplication
    for kw in &keywords {
        body_keywords.remove(kw);
    }

    // NEW: Compute term frequencies for BM25
    let mut term_frequencies: HashMap<String, usize> = HashMap::new();
    let mut total_terms = 0;

    for line in &lines {
        // Skip code blocks
        if line.starts_with("```") || line.starts_with("    ") {
            continue;
        }
        let words = extract_keywords(line);
        for word in words {
            let stemmed = stem_word(&word);
            *term_frequencies.entry(stemmed).or_insert(0) += 1;
            total_terms += 1;
        }
    }

    // NEW: Compute MinHash signature
    let all_keywords: Vec<String> = keywords.iter()
        .chain(body_keywords.iter())
        .cloned()
        .collect();
    let minhash = compute_minhash(&all_keywords, 128);

    // NEW: Compute section-level SimHash fingerprints
    let section_fingerprints = index_sections(&content, &headings);

    // Compute simhash fingerprint
    let simhash = compute_simhash(&content);

    Ok(FileEntry {
        path: path.to_string_lossy().to_string(),
        size_bytes: metadata.len(),
        line_count,
        headings,
        keywords: keywords.into_iter().collect(),
        body_keywords: body_keywords.into_iter().collect(),
        links,
        simhash,
        term_frequencies,
        doc_length: total_terms,
        minhash,
        section_fingerprints,
    })
}

fn extract_keywords(text: &str) -> Vec<String> {
    let stop_words: HashSet<&str> = [
        "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for",
        "of", "with", "by", "from", "as", "is", "was", "are", "were", "been",
        "be", "have", "has", "had", "do", "does", "did", "will", "would",
        "could", "should", "may", "might", "must", "shall", "can", "need",
        "this", "that", "these", "those", "i", "you", "he", "she", "it",
        "we", "they", "what", "which", "who", "whom", "whose", "where",
        "when", "why", "how", "all", "each", "every", "both", "few", "more",
        "most", "other", "some", "such", "no", "nor", "not", "only", "own",
        "same", "so", "than", "too", "very", "just", "also", "now", "here",
        "using", "used", "use", "new", "first", "last", "next", "then",
        "see", "get", "set", "run", "add", "create", "update", "delete",
    ].into_iter().collect();

    let word_re = Regex::new(r"[a-zA-Z][a-zA-Z0-9_-]*").unwrap();

    word_re.find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|w| w.len() >= 3 && !stop_words.contains(w.as_str()))
        .collect()
}

/// Simple suffix-stripping stemmer
fn stem_word(word: &str) -> String {
    let w = word.to_lowercase();

    // Common suffixes to strip
    let suffixes = [
        "ization", "ational", "iveness", "fulness", "ousness",
        "ation", "ement", "ment", "able", "ible", "ness", "ical",
        "ings", "ing", "ies", "ive", "ful", "ous", "ity",
        "ed", "ly", "er", "es", "s",
    ];

    for suffix in suffixes {
        if w.len() > suffix.len() + 2 && w.ends_with(suffix) {
            return w[..w.len() - suffix.len()].to_string();
        }
    }

    w
}

/// Compute simhash fingerprint for content
fn compute_simhash(content: &str) -> u64 {
    let mut v = [0i32; 64];

    // Extract features (word shingles)
    let words: Vec<&str> = content.split_whitespace().collect();

    for window in words.windows(3) {
        let shingle = format!("{} {} {}", window[0], window[1], window[2]);
        let h = hash_string(&shingle);

        for i in 0..64 {
            if (h >> i) & 1 == 1 {
                v[i] += 1;
            } else {
                v[i] -= 1;
            }
        }
    }

    // Convert to fingerprint
    let mut fingerprint: u64 = 0;
    for i in 0..64 {
        if v[i] > 0 {
            fingerprint |= 1 << i;
        }
    }

    fingerprint
}

fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Count differing bits between two simhashes (Hamming distance)
fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Convert hamming distance to similarity (0.0 to 1.0)
fn simhash_similarity(a: u64, b: u64) -> f64 {
    let distance = hamming_distance(a, b);
    1.0 - (distance as f64 / 64.0)
}

/// Index sections of a document with SimHash fingerprints
fn index_sections(content: &str, headings: &[Heading]) -> Vec<SectionFingerprint> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();

    if headings.is_empty() {
        return sections;
    }

    for i in 0..headings.len() {
        let start = headings[i].line.saturating_sub(1);
        let end = headings.get(i + 1)
            .map(|h| h.line.saturating_sub(1))
            .unwrap_or(lines.len());

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
fn compute_minhash(keywords: &[String], num_hashes: usize) -> Vec<u64> {
    let mut hashes = vec![u64::MAX; num_hashes];

    for keyword in keywords {
        for i in 0..num_hashes {
            let mut hasher = AHasher::default();
            keyword.hash(&mut hasher);
            i.hash(&mut hasher); // Use index as seed
            let h = hasher.finish();

            hashes[i] = hashes[i].min(h);
        }
    }

    hashes
}

/// Compute MinHash similarity (Jaccard estimate)
fn minhash_similarity(a: &[u64], b: &[u64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let matches = a.iter()
        .zip(b.iter())
        .filter(|(x, y)| x == y)
        .count();

    matches as f64 / a.len() as f64
}

/// Compute BM25 score for a document given query terms
fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    if doc.doc_length == 0 {
        return 0.0;
    }

    let mut score = 0.0;
    let norm_factor = 1.0 - B + B * (doc.doc_length as f64 / avg_doc_length);

    for term in query_terms {
        let stemmed = stem_word(&term.to_lowercase());
        let tf = *doc.term_frequencies.get(&stemmed).unwrap_or(&0) as f64;
        let idf = idf_map.get(&stemmed).unwrap_or(&0.0);

        if tf > 0.0 {
            score += idf * (tf * (K1 + 1.0)) / (tf + K1 * norm_factor);
        }
    }

    score
}

/// Build LSH buckets for fast duplicate detection
fn lsh_buckets(
    files: &HashMap<String, FileEntry>,
    bands: usize,
) -> HashMap<u64, Vec<String>> {
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

            buckets.entry(band_hash)
                .or_insert_with(Vec::new)
                .push(path.clone());
        }
    }

    buckets
}

fn cmd_query(
    terms: &[String],
    limit: usize,
    files_only: bool,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let _reverse_index = load_reverse_index(index_dir)?;
    let forward_index = load_forward_index(index_dir)?;

    // Compute BM25 scores for all documents
    let mut file_scores: Vec<(String, f64)> = forward_index.files.iter()
        .map(|(path, entry)| {
            let score = bm25_score(terms, entry, forward_index.avg_doc_length, &forward_index.idf_map);
            (path.clone(), score)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect();

    // Sort by BM25 score (descending)
    file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    file_scores.truncate(limit);

    let results = file_scores;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("{}", "No results found.".yellow());
        return Ok(());
    }

    println!("{} results for: {}\n",
        results.len().to_string().green().bold(),
        terms.join(" ").cyan()
    );

    for (file, score) in results {
        if files_only {
            println!("{}", file);
        } else {
            println!("{} (score: {:.2})", file.cyan(), score);

            // Show matching headings
            if let Some(entry) = forward_index.files.get(&file) {
                for heading in entry.headings.iter().take(3) {
                    let heading_keywords: HashSet<String> =
                        extract_keywords(&heading.text).into_iter()
                            .map(|k| stem_word(&k))
                            .collect();

                    let matches: Vec<_> = terms.iter()
                        .filter(|t| heading_keywords.contains(&stem_word(&t.to_lowercase())))
                        .collect();

                    if !matches.is_empty() {
                        println!("  {} L{}: {}",
                            ">".dimmed(),
                            heading.line.to_string().dimmed(),
                            heading.text
                        );
                    }
                }
            }
            println!();
        }
    }

    Ok(())
}

fn cmd_similar(
    file: &Path,
    limit: usize,
    threshold: f64,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Get keywords for reference file - try multiple path formats
    let file_str = file.to_string_lossy().to_string();
    let file_with_dot = format!("./{}", file_str.trim_start_matches("./"));
    let file_without_dot = file_str.trim_start_matches("./").to_string();

    let (matched_path, ref_entry) = forward_index.files.get(&file_str)
        .map(|e| (file_str.clone(), e))
        .or_else(|| forward_index.files.get(&file_with_dot).map(|e| (file_with_dot.clone(), e)))
        .or_else(|| forward_index.files.get(&file_without_dot).map(|e| (file_without_dot.clone(), e)))
        .ok_or_else(|| format!("File not in index: {}", file_str))?;

    // Combine heading and body keywords
    let ref_keywords: HashSet<String> = ref_entry.keywords.iter()
        .chain(ref_entry.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();

    // Compare with all other files using both Jaccard and Simhash
    let mut similarities: Vec<(String, f64, f64, f64)> = Vec::new();  // (path, jaccard, simhash, combined)

    for (path, entry) in &forward_index.files {
        if path == &matched_path {
            continue;
        }

        let other_keywords: HashSet<String> = entry.keywords.iter()
            .chain(entry.body_keywords.iter())
            .map(|k| k.to_lowercase())
            .collect();

        let jaccard = jaccard_similarity(&ref_keywords, &other_keywords);
        let simhash_sim = simhash_similarity(ref_entry.simhash, entry.simhash);

        // Combined score: weighted average
        let combined = jaccard * 0.6 + simhash_sim * 0.4;

        if combined >= threshold {
            similarities.push((path.clone(), jaccard, simhash_sim, combined));
        }
    }

    // Sort by combined similarity
    similarities.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    similarities.truncate(limit);

    if json {
        let output: Vec<_> = similarities.iter()
            .map(|(p, j, s, c)| serde_json::json!({
                "path": p,
                "jaccard": j,
                "simhash": s,
                "combined": c
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if similarities.is_empty() {
        println!("{}", "No similar files found.".yellow());
        return Ok(());
    }

    println!("Files similar to: {}\n", matched_path.cyan());
    println!("{:>5} {:>5} {:>5}  {}", "Comb", "Jacc", "Sim", "Path");
    println!("{}", "-".repeat(60));

    for (path, jaccard, simhash_sim, combined) in similarities {
        let comb_pct = (combined * 100.0) as u32;
        let jacc_pct = (jaccard * 100.0) as u32;
        let sim_pct = (simhash_sim * 100.0) as u32;
        println!("{:>4}% {:>4}% {:>4}%  {}",
            comb_pct.to_string().green(),
            jacc_pct.to_string().cyan(),
            sim_pct.to_string().yellow(),
            path
        );
    }

    Ok(())
}

fn cmd_dupes(
    threshold: f64,
    group: bool,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let start = Instant::now();

    // Build LSH buckets for fast duplicate detection
    let buckets = lsh_buckets(&forward_index.files, 16); // 16 bands x 8 rows = 128 hashes
    let mut candidates: HashSet<(String, String)> = HashSet::new();

    // Collect candidate pairs from buckets
    for paths in buckets.values() {
        if paths.len() > 1 {
            for i in 0..paths.len() {
                for j in (i + 1)..paths.len() {
                    let (p1, p2) = if paths[i] < paths[j] {
                        (paths[i].clone(), paths[j].clone())
                    } else {
                        (paths[j].clone(), paths[i].clone())
                    };
                    candidates.insert((p1, p2));
                }
            }
        }
    }

    let mut duplicates: Vec<(String, String, f64, f64, f64, f64)> = Vec::new();  // (path1, path2, jaccard, simhash, minhash, combined)

    // Compare candidate pairs
    for (path1, path2) in &candidates {
        if let (Some(entry1), Some(entry2)) = (forward_index.files.get(path1), forward_index.files.get(path2)) {
            let kw1: HashSet<String> = entry1.keywords.iter()
                .chain(entry1.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();
            let kw2: HashSet<String> = entry2.keywords.iter()
                .chain(entry2.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();

            let jaccard = jaccard_similarity(&kw1, &kw2);
            let simhash_sim = simhash_similarity(entry1.simhash, entry2.simhash);
            let minhash_sim = minhash_similarity(&entry1.minhash, &entry2.minhash);
            let combined = jaccard * 0.4 + simhash_sim * 0.3 + minhash_sim * 0.3;

            if combined >= threshold {
                duplicates.push((path1.clone(), path2.clone(), jaccard, simhash_sim, minhash_sim, combined));
            }
        }
    }

    let elapsed = start.elapsed();

    // Sort by combined similarity
    duplicates.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap_or(std::cmp::Ordering::Equal));

    if json {
        let output: Vec<_> = duplicates.iter()
            .map(|(p1, p2, j, s, m, c)| serde_json::json!({
                "file1": p1,
                "file2": p2,
                "jaccard": j,
                "simhash": s,
                "minhash": m,
                "combined": c
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if duplicates.is_empty() {
        println!("{}", "No duplicates found above threshold.".green());
        eprintln!("LSH duplicate detection: {:?} ({} candidate pairs from {} buckets)",
            elapsed,
            candidates.len(),
            buckets.len()
        );
        return Ok(());
    }

    println!("{} duplicate pairs found (threshold: {}%)",
        duplicates.len().to_string().yellow().bold(),
        (threshold * 100.0) as u32
    );
    eprintln!("LSH duplicate detection: {:?} ({} candidates from {} buckets)\n",
        elapsed,
        candidates.len(),
        buckets.len()
    );

    if group {
        // Group duplicates
        let mut groups: HashMap<String, Vec<(String, f64)>> = HashMap::new();

        for (path1, path2, _, _, _, combined) in &duplicates {
            let group = groups.entry(path1.clone()).or_insert_with(Vec::new);
            if !group.iter().any(|(p, _)| p == path2) {
                group.push((path2.clone(), *combined));
            }
        }

        for (file, related) in groups {
            println!("{}", file.cyan());
            for (r, sim) in related {
                println!("  {} {}% {}", "~".dimmed(), (sim * 100.0) as u32, r);
            }
            println!();
        }
    } else {
        for (path1, path2, jaccard, simhash_sim, minhash_sim, combined) in duplicates.iter().take(50) {
            let comb_pct = (combined * 100.0) as u32;
            println!("{}% [J:{}% S:{}% M:{}%] {} <-> {}",
                comb_pct.to_string().yellow(),
                (jaccard * 100.0) as u32,
                (simhash_sim * 100.0) as u32,
                (minhash_sim * 100.0) as u32,
                path1.cyan(),
                path2
            );
        }

        if duplicates.len() > 50 {
            println!("\n{}", format!("... and {} more", duplicates.len() - 50).dimmed());
        }
    }

    Ok(())
}

/// NEW: Show what's shared between two files
fn cmd_diff(
    file1: &Path,
    file2: &Path,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Resolve paths
    let resolve_path = |f: &Path| -> Option<(String, &FileEntry)> {
        let s = f.to_string_lossy().to_string();
        let with_dot = format!("./{}", s.trim_start_matches("./"));
        let without_dot = s.trim_start_matches("./").to_string();

        forward_index.files.get(&s).map(|e| (s.clone(), e))
            .or_else(|| forward_index.files.get(&with_dot).map(|e| (with_dot.clone(), e)))
            .or_else(|| forward_index.files.get(&without_dot).map(|e| (without_dot, e)))
    };

    let (path1, entry1) = resolve_path(file1)
        .ok_or_else(|| format!("File not in index: {}", file1.display()))?;
    let (path2, entry2) = resolve_path(file2)
        .ok_or_else(|| format!("File not in index: {}", file2.display()))?;

    // Compute similarities
    let kw1: HashSet<String> = entry1.keywords.iter()
        .chain(entry1.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();
    let kw2: HashSet<String> = entry2.keywords.iter()
        .chain(entry2.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();

    let shared: HashSet<_> = kw1.intersection(&kw2).cloned().collect();
    let only_in_1: HashSet<_> = kw1.difference(&kw2).cloned().collect();
    let only_in_2: HashSet<_> = kw2.difference(&kw1).cloned().collect();

    let jaccard = jaccard_similarity(&kw1, &kw2);
    let simhash_sim = simhash_similarity(entry1.simhash, entry2.simhash);
    let combined = jaccard * 0.6 + simhash_sim * 0.4;

    println!("{}", "Comparison".green().bold());
    println!();
    println!("  File 1: {}", path1.cyan());
    println!("  File 2: {}", path2.cyan());
    println!();
    println!("{}", "Similarity Scores".green().bold());
    println!();
    println!("  Combined:    {}%", (combined * 100.0) as u32);
    println!("  Jaccard:     {}% (keyword overlap)", (jaccard * 100.0) as u32);
    println!("  SimHash:     {}% (content structure)", (simhash_sim * 100.0) as u32);
    println!();

    println!("{} ({} keywords)", "Shared Keywords".green().bold(), shared.len());
    let mut shared_vec: Vec<_> = shared.iter().collect();
    shared_vec.sort();
    for chunk in shared_vec.chunks(8) {
        println!("  {}", chunk.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }

    println!();
    println!("{} ({} keywords)", format!("Only in {}", path1.split('/').last().unwrap_or(&path1)).yellow().bold(), only_in_1.len());
    let mut only1_vec: Vec<_> = only_in_1.iter().take(24).collect();
    only1_vec.sort();
    for chunk in only1_vec.chunks(8) {
        println!("  {}", chunk.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    if only_in_1.len() > 24 {
        println!("  ... and {} more", only_in_1.len() - 24);
    }

    println!();
    println!("{} ({} keywords)", format!("Only in {}", path2.split('/').last().unwrap_or(&path2)).yellow().bold(), only_in_2.len());
    let mut only2_vec: Vec<_> = only_in_2.iter().take(24).collect();
    only2_vec.sort();
    for chunk in only2_vec.chunks(8) {
        println!("  {}", chunk.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    if only_in_2.len() > 24 {
        println!("  ... and {} more", only_in_2.len() - 24);
    }

    // Show shared headings
    let h1: HashSet<String> = entry1.headings.iter().map(|h| h.text.to_lowercase()).collect();
    let h2: HashSet<String> = entry2.headings.iter().map(|h| h.text.to_lowercase()).collect();
    let shared_headings: Vec<_> = h1.intersection(&h2).collect();

    if !shared_headings.is_empty() {
        println!();
        println!("{} ({} headings)", "Identical Headings".red().bold(), shared_headings.len());
        for h in shared_headings.iter().take(10) {
            println!("  - {}", h);
        }
        if shared_headings.len() > 10 {
            println!("  ... and {} more", shared_headings.len() - 10);
        }
    }

    Ok(())
}

/// Find duplicate sections across documents
fn cmd_dupes_sections(
    threshold: f64,
    min_files: usize,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let start = Instant::now();

    // Collect all sections from all files
    #[derive(Debug, Clone)]
    struct SectionInfo {
        file_path: String,
        heading: String,
        line_start: usize,
        line_end: usize,
        simhash: u64,
    }

    let mut all_sections: Vec<SectionInfo> = Vec::new();
    for (path, entry) in &forward_index.files {
        for section in &entry.section_fingerprints {
            all_sections.push(SectionInfo {
                file_path: path.clone(),
                heading: section.heading.clone(),
                line_start: section.line_start,
                line_end: section.line_end,
                simhash: section.simhash,
            });
        }
    }

    if all_sections.is_empty() {
        println!("{}", "No sections found in indexed files.".yellow());
        return Ok(());
    }

    // Group similar sections using SimHash similarity
    #[derive(Debug)]
    struct SectionCluster {
        heading: String,
        files: Vec<(String, f64, usize, usize)>,  // (file_path, similarity, line_start, line_end)
        avg_simhash: u64,
    }

    let mut clusters: Vec<SectionCluster> = Vec::new();

    for section in all_sections.iter() {
        let mut best_cluster_idx: Option<usize> = None;
        let mut best_similarity = 0.0;

        // Find best matching cluster
        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            let similarity = simhash_similarity(section.simhash, cluster.avg_simhash);
            if similarity >= threshold && similarity > best_similarity {
                best_similarity = similarity;
                best_cluster_idx = Some(cluster_idx);
            }
        }

        if let Some(cluster_idx) = best_cluster_idx {
            // Add to existing cluster
            clusters[cluster_idx].files.push((
                section.file_path.clone(),
                best_similarity,
                section.line_start,
                section.line_end,
            ));
        } else {
            // Create new cluster
            clusters.push(SectionCluster {
                heading: section.heading.clone(),
                files: vec![(
                    section.file_path.clone(),
                    1.0,
                    section.line_start,
                    section.line_end,
                )],
                avg_simhash: section.simhash,
            });
        }
    }

    let elapsed = start.elapsed();

    // Filter clusters by min_files threshold
    let duplicate_clusters: Vec<_> = clusters.into_iter()
        .filter(|c| c.files.len() >= min_files)
        .collect();

    if duplicate_clusters.is_empty() {
        println!("{}", format!("No duplicate sections found with {} or more files at {}% threshold.",
            min_files, (threshold * 100.0) as u32).green());
        eprintln!("Section analysis: {:?} ({} sections analyzed)", elapsed, all_sections.len());
        return Ok(());
    }

    // Sort clusters by number of files (descending)
    let mut sorted_clusters = duplicate_clusters;
    sorted_clusters.sort_by(|a, b| b.files.len().cmp(&a.files.len()));

    if json {
        let output: Vec<_> = sorted_clusters.iter()
            .map(|cluster| serde_json::json!({
                "heading": cluster.heading,
                "file_count": cluster.files.len(),
                "files": cluster.files.iter().map(|(path, sim, start, end)| {
                    serde_json::json!({
                        "path": path,
                        "similarity": sim,
                        "line_start": start,
                        "line_end": end,
                    })
                }).collect::<Vec<_>>(),
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{} duplicate section clusters found (threshold: {}%, min files: {})",
        sorted_clusters.len().to_string().yellow().bold(),
        (threshold * 100.0) as u32,
        min_files
    );
    eprintln!("Section analysis: {:?} ({} sections analyzed)\n", elapsed, all_sections.len());

    for cluster in sorted_clusters.iter().take(20) {
        println!("{} {} ({} files)",
            "Section:".cyan().bold(),
            cluster.heading.yellow(),
            cluster.files.len()
        );

        for (path, similarity, line_start, line_end) in &cluster.files {
            let sim_pct = (similarity * 100.0) as u32;
            println!("  {}% {}:{}-{}",
                sim_pct.to_string().dimmed(),
                path,
                line_start,
                line_end
            );
        }
        println!();
    }

    if sorted_clusters.len() > 20 {
        println!("{}", format!("... and {} more section clusters", sorted_clusters.len() - 20).dimmed());
    }

    Ok(())
}

fn cmd_stats(top_keywords: usize, index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let reverse_index = load_reverse_index(index_dir)?;

    // Count keyword occurrences
    let mut keyword_counts: Vec<_> = reverse_index.keywords.iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    keyword_counts.sort_by(|a, b| b.1.cmp(&a.1));

    let total_headings: usize = forward_index.files.values()
        .map(|e| e.headings.len())
        .sum();
    let total_links: usize = forward_index.files.values()
        .map(|e| e.links.len())
        .sum();
    let total_body_keywords: usize = forward_index.files.values()
        .map(|e| e.body_keywords.len())
        .sum();

    println!("{}", "Index Statistics".green().bold());
    println!();
    println!("  Total files:       {}", forward_index.files.len().to_string().cyan());
    println!("  Unique keywords:   {}", reverse_index.keywords.len().to_string().cyan());
    println!("  Total headings:    {}", total_headings.to_string().cyan());
    println!("  Body keywords:     {}", total_body_keywords.to_string().cyan());
    println!("  Total links:       {}", total_links.to_string().cyan());
    println!("  Index version:     {}", forward_index.version.to_string().dimmed());
    println!("  Indexed at:        {}", forward_index.indexed_at.dimmed());
    println!();
    println!("{}", format!("Top {} Keywords", top_keywords).green().bold());
    println!();

    for (keyword, count) in keyword_counts.iter().take(top_keywords) {
        let bar = "=".repeat((count / 2).min(40));
        println!("  {:>20} {:>4} {}", keyword.cyan(), count, bar.dimmed());
    }

    Ok(())
}

fn cmd_repl(index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "yore interactive mode (v2)".green().bold());
    println!("Commands: query <terms>, similar <file>, dupes, diff <f1> <f2>, stats, help, quit\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("{} ", ">".cyan().bold());
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "quit" | "exit" | "q" => break,
            "help" | "?" => {
                println!("  query <terms...>   - Search for keywords");
                println!("  similar <file>     - Find similar files");
                println!("  dupes              - Find duplicates");
                println!("  diff <f1> <f2>     - Compare two files");
                println!("  stats              - Show statistics");
                println!("  quit               - Exit");
            }
            "query" => {
                let terms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                if terms.is_empty() {
                    println!("{}", "Usage: query <terms...>".yellow());
                } else {
                    let _ = cmd_query(&terms, 10, false, false, index_dir);
                }
            }
            "similar" => {
                if parts.len() < 2 {
                    println!("{}", "Usage: similar <file>".yellow());
                } else {
                    let _ = cmd_similar(Path::new(parts[1]), 5, 0.3, false, index_dir);
                }
            }
            "dupes" => {
                let _ = cmd_dupes(0.35, false, false, index_dir);
            }
            "diff" => {
                if parts.len() < 3 {
                    println!("{}", "Usage: diff <file1> <file2>".yellow());
                } else {
                    let _ = cmd_diff(Path::new(parts[1]), Path::new(parts[2]), index_dir);
                }
            }
            "stats" => {
                let _ = cmd_stats(10, index_dir);
            }
            _ => {
                // Treat as query
                let terms: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                let _ = cmd_query(&terms, 10, false, false, index_dir);
            }
        }
        println!();
    }

    Ok(())
}

// Helper functions

fn load_forward_index(index_dir: &Path) -> Result<ForwardIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("forward_index.json");
    let content = fs::read_to_string(&path)
        .map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

fn load_reverse_index(index_dir: &Path) -> Result<ReverseIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("reverse_index.json");
    let content = fs::read_to_string(&path)
        .map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_secs())
}

// ============================================================================
// Context Assembly for LLMs (Phase 2)
// ============================================================================

#[derive(Debug, Clone)]
struct SectionMatch {
    doc_path: String,
    heading: String,
    line_start: usize,
    line_end: usize,
    bm25_score: f64,
    content: String,
    canonicality: f64,
}

/// Search for relevant sections using BM25 scoring
fn search_relevant_sections(
    query: &str,
    index: &ForwardIndex,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let query_terms: Vec<String> = query
        .split_whitespace()
        .map(|s| stem_word(&s.to_lowercase()))
        .collect();

    let mut all_sections: Vec<SectionMatch> = Vec::new();

    // First, get top documents by BM25
    let mut doc_scores: Vec<(&String, &FileEntry, f64)> = index
        .files
        .iter()
        .map(|(path, entry)| {
            let score = bm25_score(&query_terms, entry, index.avg_doc_length, &index.idf_map);
            (path, entry, score)
        })
        .filter(|(_, _, score)| *score > 0.01)
        .collect();

    doc_scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Take top 20 documents
    for (doc_path, entry, doc_score) in doc_scores.iter().take(20) {
        let canonicality = score_canonicality(doc_path, entry);

        // Split document into sections based on section_fingerprints
        if !entry.section_fingerprints.is_empty() {
            // Use indexed sections
            for section in &entry.section_fingerprints {
                // Read the actual section content
                if let Ok(content) = fs::read_to_string(doc_path) {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = section.line_start.saturating_sub(1);
                    let end = section.line_end.min(lines.len());

                    if start < end {
                        let section_content = lines[start..end].join("\n");

                        all_sections.push(SectionMatch {
                            doc_path: doc_path.to_string(),
                            heading: section.heading.clone(),
                            line_start: section.line_start,
                            line_end: section.line_end,
                            bm25_score: *doc_score, // Use doc-level score for now
                            content: section_content,
                            canonicality,
                        });
                    }
                }
            }
        } else {
            // Fallback: treat whole doc as one section
            if let Ok(content) = fs::read_to_string(doc_path) {
                all_sections.push(SectionMatch {
                    doc_path: doc_path.to_string(),
                    heading: "Full Document".to_string(),
                    line_start: 1,
                    line_end: content.lines().count(),
                    bm25_score: *doc_score,
                    content,
                    canonicality,
                });
            }
        }
    }

    // Sort by combined score: BM25 * 0.7 + canonicality * 0.3
    all_sections.sort_by(|a, b| {
        let score_a = a.bm25_score * 0.7 + a.canonicality * 0.3;
        let score_b = b.bm25_score * 0.7 + b.canonicality * 0.3;
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Take top N sections
    all_sections.into_iter().take(max_sections).collect()
}

/// Score document canonicality based on path, recency, and patterns
fn score_canonicality(doc_path: &str, entry: &FileEntry) -> f64 {
    let mut score: f64 = 0.5; // baseline

    let path_lower = doc_path.to_lowercase();

    // Path-based boosts
    if path_lower.contains("docs/adr/") || path_lower.contains("docs/architecture/") {
        score += 0.2;
    }
    if path_lower.contains("docs/index/") {
        score += 0.15;
    }
    if path_lower.contains("scratch") || path_lower.contains("archive") || path_lower.contains("old") {
        score -= 0.3;
    }
    if path_lower.contains("deprecated") || path_lower.contains("backup") {
        score -= 0.25;
    }

    // Filename patterns
    let filename = Path::new(doc_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if filename.contains("readme") || filename.contains("index") {
        score += 0.1;
    }
    if filename.contains("guide") || filename.contains("runbook") || filename.contains("plan") {
        score += 0.1;
    }

    // Recency (approximate - we don't have mtime in index yet)
    // For now, we'll just use this as a placeholder
    // In future: add last_modified to FileEntry

    // Clamp to [0.0, 1.0]
    score.max(0.0).min(1.0)
}

/// Distill sections into markdown digest within token budget
fn distill_to_markdown(
    sections: &[SectionMatch],
    query: &str,
    max_tokens: usize,
) -> String {
    let mut output = String::new();
    let mut used_tokens = 0;

    // Header
    let header = format!(
        "# Context Digest for: \"{}\"\n\n\
         **Generated:** {}\n\
         **Token Budget:** {}\n\
         **Documents Scanned:** N/A\n\
         **Sections Selected:** {}\n\n\
         ---\n\n",
        query,
        chrono_now(),
        max_tokens,
        sections.len()
    );
    output.push_str(&header);
    used_tokens += estimate_tokens(&header);

    // Group sections by document
    let mut doc_groups: HashMap<String, Vec<&SectionMatch>> = HashMap::new();
    for section in sections {
        doc_groups
            .entry(section.doc_path.clone())
            .or_insert_with(Vec::new)
            .push(section);
    }

    // Top Relevant Documents section
    output.push_str("## Top Relevant Documents\n\n");
    used_tokens += 10;

    let mut ranked_docs: Vec<_> = doc_groups.iter().collect();
    ranked_docs.sort_by(|a, b| {
        let score_a = a.1[0].bm25_score * 0.7 + a.1[0].canonicality * 0.3;
        let score_b = b.1[0].bm25_score * 0.7 + b.1[0].canonicality * 0.3;
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (idx, (doc_path, doc_sections)) in ranked_docs.iter().enumerate().take(10) {
        let section = doc_sections[0];
        let combined_score = section.bm25_score * 0.7 + section.canonicality * 0.3;
        let doc_line = format!(
            "{}. **{}** (score: {:.2}, canonical: {:.2})\n   - Sections included: {}\n\n",
            idx + 1,
            doc_path,
            combined_score,
            section.canonicality,
            doc_sections.len()
        );
        output.push_str(&doc_line);
        used_tokens += estimate_tokens(&doc_line);
    }

    output.push_str("---\n\n## Distilled Content\n\n");
    used_tokens += 10;

    // Add sections
    for section in sections {
        if used_tokens >= max_tokens {
            output.push_str("\n\n*[Content truncated due to token budget]*\n");
            break;
        }

        let section_header = format!(
            "### {} (from {})\n\n**Source:** {}:{}-{} (canonical: {:.2})\n\n",
            section.heading,
            section.doc_path,
            section.doc_path,
            section.line_start,
            section.line_end,
            section.canonicality
        );

        // Estimate how much space we need
        let section_tokens = estimate_tokens(&section_header) + estimate_tokens(&section.content);

        if used_tokens + section_tokens > max_tokens {
            // Try to fit a truncated version
            let remaining_tokens = max_tokens - used_tokens;
            let chars_to_include = remaining_tokens * 4; // rough approximation

            if chars_to_include > 200 {
                output.push_str(&section_header);
                output.push_str(&section.content[..chars_to_include.min(section.content.len())]);
                output.push_str("\n\n*[Section truncated]*\n");
            }
            break;
        }

        output.push_str(&section_header);
        output.push_str(&section.content);
        output.push_str("\n\n---\n\n");

        used_tokens += section_tokens;
    }

    // Metadata footer
    let footer = format!(
        "\n## Metadata\n\n\
         **Canonicality Scores:**\n\
         - 0.90+: Authoritative source, prefer over other docs\n\
         - 0.70-0.89: Reliable, current documentation\n\
         - 0.50-0.69: Secondary or supporting documentation\n\
         - <0.50: Potentially stale, use with caution\n\n\
         **Actual Tokens Used:** ~{}\n\n\
         ---\n\n\
         ## Usage with LLM\n\n\
         Paste this digest into your LLM conversation, then ask:\n\n\
         > Using only the information in the context above, answer: \"{}\"\n\
         > Be explicit when something is not documented in the context.\n",
        used_tokens, query
    );

    output.push_str(&footer);

    output
}

/// Estimate token count (rough approximation: 1 token ≈ 4 chars)
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Main assemble command handler
fn cmd_assemble(
    query: &str,
    max_tokens: usize,
    max_sections: usize,
    _depth: usize, // TODO: implement cross-reference expansion
    format: &str,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if format != "markdown" {
        return Err("Only markdown format is supported currently".into());
    }

    let forward_index = load_forward_index(index_dir)?;

    // Search for relevant sections
    let sections = search_relevant_sections(query, &forward_index, max_sections);

    if sections.is_empty() {
        println!("# No relevant sections found for query: \"{}\"", query);
        return Ok(());
    }

    // Distill to markdown
    let digest = distill_to_markdown(&sections, query, max_tokens);

    println!("{}", digest);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jaccard_similarity() {
        let set1: HashSet<String> = ["foo", "bar", "baz"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let set2: HashSet<String> = ["bar", "baz", "qux"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let sim = jaccard_similarity(&set1, &set2);
        // Intersection: {bar, baz} = 2
        // Union: {foo, bar, baz, qux} = 4
        // Jaccard: 2/4 = 0.5
        assert_eq!(sim, 0.5);

        // Empty sets
        let empty1: HashSet<String> = HashSet::new();
        let empty2: HashSet<String> = HashSet::new();
        assert_eq!(jaccard_similarity(&empty1, &empty2), 0.0);

        // Identical sets
        assert_eq!(jaccard_similarity(&set1, &set1), 1.0);
    }

    #[test]
    fn test_simhash_similarity() {
        // Identical hashes
        assert_eq!(simhash_similarity(0x123456, 0x123456), 1.0);

        // Completely different (all bits flipped)
        let hash1 = 0x0000000000000000u64;
        let hash2 = 0xFFFFFFFFFFFFFFFFu64;
        assert_eq!(simhash_similarity(hash1, hash2), 0.0);

        // 1 bit different out of 64
        let hash_a = 0b0000000000000000u64;
        let hash_b = 0b0000000000000001u64;
        let sim = simhash_similarity(hash_a, hash_b);
        assert!((sim - (63.0 / 64.0)).abs() < 0.01);
    }

    #[test]
    fn test_hamming_distance() {
        assert_eq!(hamming_distance(0b1010, 0b1010), 0);
        assert_eq!(hamming_distance(0b1010, 0b0101), 4);
        assert_eq!(hamming_distance(0b1111, 0b0000), 4);
        assert_eq!(hamming_distance(0b1100, 0b1010), 2);
    }

    #[test]
    fn test_compute_simhash_stability() {
        let text1 = "The quick brown fox jumps over the lazy dog";
        let text2 = "The quick brown fox jumps over the lazy dog";

        let hash1 = compute_simhash(text1);
        let hash2 = compute_simhash(text2);

        // Identical text should produce identical hashes
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_simhash_similarity() {
        let text1 = "machine learning algorithms";
        let text2 = "machine learning systems";
        let text3 = "completely different topic about cooking";

        let hash1 = compute_simhash(text1);
        let hash2 = compute_simhash(text2);
        let hash3 = compute_simhash(text3);

        // Similar texts should have high similarity
        let sim_similar = simhash_similarity(hash1, hash2);
        // Different texts should have lower similarity
        let sim_different = simhash_similarity(hash1, hash3);

        assert!(sim_similar > sim_different);
        assert!(sim_similar > 0.5); // Similar texts should be > 50% similar
    }

    #[test]
    fn test_minhash_basic() {
        let keywords1 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
        let keywords2 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];

        let mh1 = compute_minhash(&keywords1, 128);
        let mh2 = compute_minhash(&keywords2, 128);

        // Same keywords should produce same MinHash
        assert_eq!(mh1, mh2);
        assert_eq!(mh1.len(), 128);

        // Similarity should be 1.0
        assert_eq!(minhash_similarity(&mh1, &mh2), 1.0);
    }

    #[test]
    fn test_minhash_similarity_estimation() {
        let keywords1 = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let keywords2 = vec!["b".to_string(), "c".to_string(), "d".to_string()];
        let keywords3 = vec!["x".to_string(), "y".to_string(), "z".to_string()];

        let mh1 = compute_minhash(&keywords1, 128);
        let mh2 = compute_minhash(&keywords2, 128);
        let mh3 = compute_minhash(&keywords3, 128);

        // keywords1 and keywords2 share 2 out of 4 unique items = 0.5 Jaccard
        let sim_similar = minhash_similarity(&mh1, &mh2);
        // keywords1 and keywords3 share 0 items
        let sim_different = minhash_similarity(&mh1, &mh3);

        // Similar sets should have higher MinHash similarity
        assert!(sim_similar > sim_different);
        // MinHash should approximate Jaccard (within reasonable error)
        assert!(sim_similar > 0.3 && sim_similar < 0.7); // Approximately 0.5
    }

    #[test]
    fn test_lsh_buckets() {
        let mut files = HashMap::new();

        // Create 3 files with MinHash signatures
        let keywords1 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
        let keywords2 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
        let keywords3 = vec!["completely".to_string(), "different".to_string()];

        files.insert(
            "file1.md".to_string(),
            FileEntry {
                path: "file1.md".to_string(),
                size_bytes: 100,
                line_count: 10,
                headings: vec![],
                keywords: keywords1.clone(),
                body_keywords: vec![],
                links: vec![],
                simhash: 0,
                term_frequencies: HashMap::new(),
                doc_length: 0,
                minhash: compute_minhash(&keywords1, 128),
                section_fingerprints: vec![],
            },
        );

        files.insert(
            "file2.md".to_string(),
            FileEntry {
                path: "file2.md".to_string(),
                size_bytes: 100,
                line_count: 10,
                headings: vec![],
                keywords: keywords2.clone(),
                body_keywords: vec![],
                links: vec![],
                simhash: 0,
                term_frequencies: HashMap::new(),
                doc_length: 0,
                minhash: compute_minhash(&keywords2, 128),
                section_fingerprints: vec![],
            },
        );

        files.insert(
            "file3.md".to_string(),
            FileEntry {
                path: "file3.md".to_string(),
                size_bytes: 100,
                line_count: 10,
                headings: vec![],
                keywords: keywords3.clone(),
                body_keywords: vec![],
                links: vec![],
                simhash: 0,
                term_frequencies: HashMap::new(),
                doc_length: 0,
                minhash: compute_minhash(&keywords3, 128),
                section_fingerprints: vec![],
            },
        );

        let buckets = lsh_buckets(&files, 16);

        // Should create some buckets
        assert!(!buckets.is_empty());

        // file1 and file2 should likely be in the same bucket (identical MinHash)
        // Check if they appear together in any bucket
        let mut file1_file2_together = false;
        for paths in buckets.values() {
            if paths.contains(&"file1.md".to_string()) && paths.contains(&"file2.md".to_string()) {
                file1_file2_together = true;
                break;
            }
        }
        assert!(file1_file2_together, "Identical files should be in same LSH bucket");
    }

    #[test]
    fn test_bm25_score_basic() {
        let mut term_freq = HashMap::new();
        term_freq.insert("test".to_string(), 5);
        term_freq.insert("word".to_string(), 2);

        let doc = FileEntry {
            path: "test.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: vec![],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: term_freq,
            doc_length: 100,
            minhash: vec![],
            section_fingerprints: vec![],
        };

        let mut idf_map = HashMap::new();
        idf_map.insert("test".to_string(), 2.5);
        idf_map.insert("word".to_string(), 1.8);

        let query = vec!["test".to_string()];
        let score = bm25_score(&query, &doc, 100.0, &idf_map);

        // Score should be > 0 for matching term
        assert!(score > 0.0);

        // Query with no matching terms should score 0
        let empty_query = vec!["nonexistent".to_string()];
        let zero_score = bm25_score(&empty_query, &doc, 100.0, &idf_map);
        assert_eq!(zero_score, 0.0);
    }

    #[test]
    fn test_bm25_score_ordering() {
        // Document with high term frequency
        let mut tf_high = HashMap::new();
        tf_high.insert("test".to_string(), 10);

        let doc_high_tf = FileEntry {
            path: "high.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: vec![],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: tf_high,
            doc_length: 50,
            minhash: vec![],
            section_fingerprints: vec![],
        };

        // Document with low term frequency
        let mut tf_low = HashMap::new();
        tf_low.insert("test".to_string(), 1);

        let doc_low_tf = FileEntry {
            path: "low.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: vec![],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: tf_low,
            doc_length: 50,
            minhash: vec![],
            section_fingerprints: vec![],
        };

        let mut idf_map = HashMap::new();
        idf_map.insert("test".to_string(), 2.0);

        let query = vec!["test".to_string()];
        let score_high = bm25_score(&query, &doc_high_tf, 50.0, &idf_map);
        let score_low = bm25_score(&query, &doc_low_tf, 50.0, &idf_map);

        // Higher term frequency should yield higher BM25 score
        assert!(score_high > score_low);
    }

    #[test]
    fn test_index_sections() {
        let content = "# Introduction\nThis is the intro.\n\n## Details\nMore details here.\n\n## Summary\nFinal thoughts.";
        let headings = vec![
            Heading {
                line: 1,
                level: 1,
                text: "Introduction".to_string(),
            },
            Heading {
                line: 4,
                level: 2,
                text: "Details".to_string(),
            },
            Heading {
                line: 7,
                level: 2,
                text: "Summary".to_string(),
            },
        ];

        let sections = index_sections(content, &headings);

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].heading, "Introduction");
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[0].line_start, 1);

        assert_eq!(sections[1].heading, "Details");
        assert_eq!(sections[1].level, 2);
        assert_eq!(sections[1].line_start, 4);

        assert_eq!(sections[2].heading, "Summary");
        assert_eq!(sections[2].level, 2);
    }

    #[test]
    fn test_index_sections_similar_content() {
        let content1 = "## Testing\nRun the tests with:\n```\npytest\n```";
        let content2 = "## Testing\nRun the tests with:\n```\npytest\n```";
        let content3 = "## Testing\nCompletely different content about testing";

        let headings1 = vec![Heading { line: 1, level: 2, text: "Testing".to_string() }];
        let headings2 = vec![Heading { line: 1, level: 2, text: "Testing".to_string() }];
        let headings3 = vec![Heading { line: 1, level: 2, text: "Testing".to_string() }];

        let sections1 = index_sections(content1, &headings1);
        let sections2 = index_sections(content2, &headings2);
        let sections3 = index_sections(content3, &headings3);

        // Identical content should produce identical SimHash
        assert_eq!(sections1[0].simhash, sections2[0].simhash);

        // Different content should produce different SimHash
        assert_ne!(sections1[0].simhash, sections3[0].simhash);

        // Identical sections should have 100% similarity
        let sim_identical = simhash_similarity(sections1[0].simhash, sections2[0].simhash);
        assert_eq!(sim_identical, 1.0);

        // Different sections should have < 100% similarity
        let sim_different = simhash_similarity(sections1[0].simhash, sections3[0].simhash);
        assert!(sim_different < 1.0);
    }

    #[test]
    fn test_extract_keywords() {
        let text = "This is a TEST document with some KEYWORDS";
        let keywords = extract_keywords(text);

        // Should lowercase (but not stem - extract_keywords doesn't stem)
        assert!(keywords.contains(&"test".to_string()));
        assert!(keywords.contains(&"document".to_string()));
        assert!(keywords.contains(&"keywords".to_string())); // Note: not stemmed

        // Should not contain stop words
        assert!(!keywords.contains(&"this".to_string()));
        assert!(!keywords.contains(&"is".to_string()));
        // "a" and "with" are too short or stop words
        assert!(!keywords.contains(&"with".to_string()));
    }

    #[test]
    fn test_stem_word() {
        // Test actual stemming behavior
        assert_eq!(stem_word("running"), "runn"); // Simple stemmer removes "ing"
        assert_eq!(stem_word("tests"), "test");   // Removes "s"
        assert_eq!(stem_word("testing"), "test"); // Removes "ing"
        assert_eq!(stem_word("keywords"), "keyword"); // Removes "s"

        // Short words should not be stemmed
        assert_eq!(stem_word("go"), "go");
        assert_eq!(stem_word("it"), "it");
    }
}
