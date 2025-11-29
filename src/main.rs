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
}

// Index structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>,  // NEW: keywords from full text
    links: Vec<Link>,
    simhash: u64,  // NEW: content fingerprint
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
    version: u32,  // NEW: index version for compatibility
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
        Commands::Diff { file1, file2, index } => {
            cmd_diff(&file1, &file2, &index)
        }
        Commands::Stats { top_keywords, index } => {
            cmd_stats(top_keywords, &index)
        }
        Commands::Repl { index } => {
            cmd_repl(&index)
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
        version: 2,  // Version 2 includes body_keywords and simhash
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

    // NEW: Compute simhash fingerprint
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

fn cmd_query(
    terms: &[String],
    limit: usize,
    files_only: bool,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let reverse_index = load_reverse_index(index_dir)?;
    let forward_index = load_forward_index(index_dir)?;

    // Find files matching all terms (with stemming)
    let mut file_scores: HashMap<String, usize> = HashMap::new();

    for term in terms {
        let stemmed = stem_word(&term.to_lowercase());
        if let Some(entries) = reverse_index.keywords.get(&stemmed) {
            for entry in entries {
                *file_scores.entry(entry.file.clone()).or_insert(0) += 1;
            }
        }
        // Also try original term
        if let Some(entries) = reverse_index.keywords.get(&term.to_lowercase()) {
            for entry in entries {
                *file_scores.entry(entry.file.clone()).or_insert(0) += 1;
            }
        }
    }

    // Sort by score
    let mut results: Vec<_> = file_scores.into_iter().collect();
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results.truncate(limit);

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
            println!("{} (score: {})", file.cyan(), score);

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

    let files: Vec<_> = forward_index.files.iter().collect();
    let mut duplicates: Vec<(String, String, f64, f64, f64)> = Vec::new();  // (path1, path2, jaccard, simhash, combined)

    // Compare all pairs
    for i in 0..files.len() {
        for j in (i + 1)..files.len() {
            let (path1, entry1) = files[i];
            let (path2, entry2) = files[j];

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
            let combined = jaccard * 0.6 + simhash_sim * 0.4;

            if combined >= threshold {
                duplicates.push((path1.clone(), path2.clone(), jaccard, simhash_sim, combined));
            }
        }
    }

    // Sort by combined similarity
    duplicates.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

    if json {
        let output: Vec<_> = duplicates.iter()
            .map(|(p1, p2, j, s, c)| serde_json::json!({
                "file1": p1,
                "file2": p2,
                "jaccard": j,
                "simhash": s,
                "combined": c
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if duplicates.is_empty() {
        println!("{}", "No duplicates found above threshold.".green());
        return Ok(());
    }

    println!("{} duplicate pairs found (threshold: {}%)\n",
        duplicates.len().to_string().yellow().bold(),
        (threshold * 100.0) as u32
    );

    if group {
        // Group duplicates
        let mut groups: HashMap<String, Vec<(String, f64)>> = HashMap::new();

        for (path1, path2, _, _, combined) in &duplicates {
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
        for (path1, path2, jaccard, simhash_sim, combined) in duplicates.iter().take(50) {
            let comb_pct = (combined * 100.0) as u32;
            println!("{}% [J:{}% S:{}%] {} <-> {}",
                comb_pct.to_string().yellow(),
                (jaccard * 100.0) as u32,
                (simhash_sim * 100.0) as u32,
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
