use crate::commands_query::*;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::assemble::*;
use crate::search::*;
use crate::types::*;
use crate::util::*;

pub(crate) fn cmd_stats(
    top_keywords: usize,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let reverse_index = load_reverse_index(index_dir)?;

    // Count keyword occurrences
    let mut keyword_counts: Vec<_> = reverse_index
        .keywords
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    keyword_counts.sort_by(|a, b| b.1.cmp(&a.1));

    let total_headings: usize = forward_index.files.values().map(|e| e.headings.len()).sum();
    let total_links: usize = forward_index.files.values().map(|e| e.links.len()).sum();
    let total_body_keywords: usize = forward_index
        .files
        .values()
        .map(|e| e.body_keywords.len())
        .sum();

    if json {
        let result = StatsResult {
            total_files: forward_index.files.len(),
            unique_keywords: reverse_index.keywords.len(),
            total_headings,
            body_keywords: total_body_keywords,
            total_links,
            index_version: forward_index.version,
            indexed_at: forward_index.indexed_at.clone(),
            top_keywords: keyword_counts
                .iter()
                .take(top_keywords)
                .map(|(k, c)| KeywordCount {
                    keyword: k.clone(),
                    count: *c,
                })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", "Index Statistics".green().bold());
    println!();
    println!(
        "  Total files:       {}",
        forward_index.files.len().to_string().cyan()
    );
    println!(
        "  Unique keywords:   {}",
        reverse_index.keywords.len().to_string().cyan()
    );
    println!("  Total headings:    {}", total_headings.to_string().cyan());
    println!(
        "  Body keywords:     {}",
        total_body_keywords.to_string().cyan()
    );
    println!("  Total links:       {}", total_links.to_string().cyan());
    println!(
        "  Index version:     {}",
        forward_index.version.to_string().dimmed()
    );
    println!("  Indexed at:        {}", forward_index.indexed_at.dimmed());
    println!();
    println!("{}", format!("Top {top_keywords} Keywords").green().bold());
    println!();

    for (keyword, count) in keyword_counts.iter().take(top_keywords) {
        let bar = "=".repeat((count / 2).min(40));
        println!("  {:>20} {:>4} {}", keyword.cyan(), count, bar.dimmed());
    }

    Ok(())
}

pub(crate) fn cmd_repl(index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "yore interactive mode (v2)".green().bold());
    println!("Commands: query <terms>, similar <file>, dupes, diff <f1> <f2>, stats, help, quit\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let query_options = QueryOptions {
        limit: 10,
        files_only: false,
        json: false,
        doc_terms: 0,
        explain: false,
        require_phrases: false,
        filter_stopwords: true,
    };

    loop {
        print!("{} ", ">".cyan().bold());
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
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
                let trimmed = line.trim();
                let rest = trimmed.strip_prefix("query").unwrap_or("").trim();
                if rest.is_empty() {
                    println!("{}", "Usage: query <terms...>".yellow());
                } else {
                    let _ = cmd_query(rest, index_dir, &query_options);
                }
            }
            "similar" => {
                if parts.len() < 2 {
                    println!("{}", "Usage: similar <file>".yellow());
                } else {
                    let _ = cmd_similar(Path::new(parts[1]), 5, 0.3, false, 0, index_dir);
                }
            }
            "dupes" => {
                let _ = cmd_dupes(0.35, false, false, index_dir);
            }
            "diff" => {
                if parts.len() < 3 {
                    println!("{}", "Usage: diff <file1> <file2>".yellow());
                } else {
                    let _ = cmd_diff(Path::new(parts[1]), Path::new(parts[2]), index_dir, false);
                }
            }
            "stats" => {
                let _ = cmd_stats(10, index_dir, false);
            }
            _ => {
                // Treat as query
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = cmd_query(trimmed, index_dir, &query_options);
                }
            }
        }
        println!();
    }

    Ok(())
}

pub(crate) fn cmd_vocabulary(
    index_dir: &Path,
    limit: usize,
    format: &str,
    json: bool,
    options: VocabularyOptions<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let reverse_index = match load_reverse_index(index_dir) {
        Ok(index) => index,
        Err(err) => {
            if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::NotFound {
                    ReverseIndex {
                        keywords: HashMap::new(),
                    }
                } else {
                    return Err(err);
                }
            } else {
                return Err(err);
            }
        }
    };

    let forward_index = load_forward_index(index_dir).ok();
    let stopwords_path = options
        .stopwords
        .map(|path| path.to_string_lossy().to_string());
    let mut stopwords =
        load_vocabulary_stopwords(options.stopwords, !options.no_default_stopwords)?;
    let auto_common_terms = if options.common_terms > 0 {
        let candidate_metrics: Vec<VocabularyCandidateTerm> = reverse_index
            .keywords
            .iter()
            .map(|(term, postings)| {
                let term = term.to_string();
                VocabularyCandidateTerm {
                    term: term.clone(),
                    surface: None,
                    term_freq: postings.len(),
                    doc_freq: postings
                        .iter()
                        .map(|posting| posting.file.clone())
                        .collect::<HashSet<_>>()
                        .len(),
                    first_file: String::new(),
                    first_line: usize::MAX,
                    first_heading: String::new(),
                }
            })
            .collect();
        let common =
            build_auto_common_vocabulary_stopwords(&candidate_metrics, options.common_terms);
        for common_term in &common {
            stopwords.insert(common_term.clone());
        }
        Some(common.len())
    } else {
        None
    };

    let mut candidates: Vec<VocabularyCandidateTerm> = reverse_index
        .keywords
        .into_iter()
        .filter(|(_, postings)| !postings.is_empty())
        .map(|(term, postings)| {
            let mut ordered_postings = postings;

            let mut docs = HashSet::new();
            for posting in &ordered_postings {
                docs.insert(posting.file.clone());
            }
            ordered_postings.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then_with(|| {
                        a.line
                            .unwrap_or(usize::MAX)
                            .cmp(&b.line.unwrap_or(usize::MAX))
                    })
                    .then_with(|| {
                        a.heading
                            .as_deref()
                            .unwrap_or("")
                            .cmp(b.heading.as_deref().unwrap_or(""))
                    })
            });

            let first = ordered_postings.first().expect("postings non-empty");
            let first_heading = first.heading.clone().unwrap_or_default();

            VocabularyCandidateTerm {
                term: term.clone(),
                surface: resolve_vocabulary_surface(
                    &term,
                    &ordered_postings,
                    forward_index.as_ref(),
                ),
                term_freq: ordered_postings.len(),
                doc_freq: docs.len(),
                first_file: first.file.clone(),
                first_line: first.line.unwrap_or(usize::MAX),
                first_heading,
            }
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.doc_freq
            .cmp(&a.doc_freq)
            .then_with(|| b.term_freq.cmp(&a.term_freq))
            .then_with(|| a.first_file.cmp(&b.first_file))
            .then_with(|| a.first_line.cmp(&b.first_line))
            .then_with(|| a.first_heading.cmp(&b.first_heading))
            .then_with(|| a.term.cmp(&b.term))
    });

    let mut terms = Vec::new();
    for candidate in &candidates {
        let term = if let Some(surface) = &candidate.surface {
            surface
        } else if options.include_stemming {
            &candidate.term
        } else {
            continue;
        };

        let term_lower = term.to_lowercase();
        if !is_hygienic_vocabulary_term(term) || stopwords.contains(&term_lower) {
            continue;
        }

        terms.push(VocabularyTerm {
            term: term.clone(),
            score: candidate.doc_freq as f64,
            count: candidate.term_freq,
        });
    }
    let (terms, total_candidates) = apply_vocabulary_limit(terms, limit);

    let effective_format = if json { "json" } else { format };
    let result = VocabularyResult {
        format: effective_format.to_string(),
        limit,
        total: total_candidates,
        terms,
        stopwords: stopwords_path,
        used_default_stopwords: !options.no_default_stopwords,
        auto_common_terms,
        include_stemming: options.include_stemming,
    };

    match effective_format {
        "lines" => {
            if !result.terms.is_empty() {
                println!("{}", render_vocabulary_lines(&result.terms));
            }
            Ok(())
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        "prompt" => {
            println!("{}", render_vocabulary_prompt(&result.terms));
            Ok(())
        }
        _ => Err(format!("Unsupported vocabulary format: {effective_format}").into()),
    }
}

pub(crate) fn render_vocabulary_prompt(terms: &[VocabularyTerm]) -> String {
    let rendered_terms: Vec<String> = terms
        .iter()
        .map(|term| normalize_prompt_term(&term.term))
        .filter(|term| !term.is_empty())
        .collect();

    rendered_terms.join(", ")
}

pub(crate) fn render_vocabulary_lines(terms: &[VocabularyTerm]) -> String {
    terms
        .iter()
        .map(|term| term.term.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn apply_vocabulary_limit(
    mut terms: Vec<VocabularyTerm>,
    limit: usize,
) -> (Vec<VocabularyTerm>, usize) {
    let total = terms.len();
    if terms.len() > limit {
        terms.truncate(limit);
    }
    (terms, total)
}

pub(crate) fn build_auto_common_vocabulary_stopwords(
    candidates: &[VocabularyCandidateTerm],
    top_n: usize,
) -> HashSet<String> {
    if top_n == 0 || candidates.is_empty() {
        return HashSet::new();
    }

    let mut candidates = candidates.to_vec();
    candidates.sort_by(|a, b| {
        b.term_freq
            .cmp(&a.term_freq)
            .then_with(|| b.doc_freq.cmp(&a.doc_freq))
            .then_with(|| a.term.cmp(&b.term))
    });

    candidates
        .into_iter()
        .filter(|candidate| is_hygienic_vocabulary_term(&candidate.term))
        .take(top_n)
        .map(|candidate| candidate.term.to_lowercase())
        .collect()
}

pub(crate) fn resolve_vocabulary_surface(
    stem: &str,
    postings: &[ReverseEntry],
    forward_index: Option<&ForwardIndex>,
) -> Option<String> {
    #[derive(Debug)]
    struct SurfaceCandidate {
        value: String,
        file: String,
        line: usize,
        source_rank: usize,
        token_idx: usize,
    }

    let mut ordered_postings = postings.to_vec();
    ordered_postings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| {
                a.line
                    .unwrap_or(usize::MAX)
                    .cmp(&b.line.unwrap_or(usize::MAX))
            })
            .then_with(|| {
                a.heading
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.heading.as_deref().unwrap_or(""))
            })
    });

    let mut candidates: Vec<SurfaceCandidate> = Vec::new();

    for posting in &ordered_postings {
        if let Some(heading) = &posting.heading {
            for (token_idx, token) in extract_keywords(heading).into_iter().enumerate() {
                if stem_word(&token) == stem {
                    candidates.push(SurfaceCandidate {
                        value: token,
                        file: posting.file.clone(),
                        line: posting.line.unwrap_or(usize::MAX),
                        source_rank: 0,
                        token_idx,
                    });
                }
            }
        }

        if let Some(forward_index) = forward_index {
            if let Some(entry) = forward_index.files.get(&posting.file) {
                for (token_idx, token) in entry
                    .keywords
                    .iter()
                    .chain(entry.body_keywords.iter())
                    .enumerate()
                {
                    if stem_word(&token.to_lowercase()) == stem {
                        candidates.push(SurfaceCandidate {
                            value: token.to_lowercase(),
                            file: posting.file.clone(),
                            line: posting.line.unwrap_or(usize::MAX),
                            source_rank: 1,
                            token_idx,
                        });
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        a.source_rank
            .cmp(&b.source_rank)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.token_idx.cmp(&b.token_idx))
            .then_with(|| a.value.cmp(&b.value))
    });

    candidates.first().map(|candidate| candidate.value.clone())
}

pub(crate) fn normalize_prompt_term(term: &str) -> String {
    let no_control: String = term
        .chars()
        .filter(|character| !character.is_control())
        .collect();

    no_control
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Main assemble command handler
pub(crate) fn cmd_assemble(
    query: &str,
    from_files: &[String],
    options: &AssembleOptions,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if options.format != "markdown" {
        return Err("Only markdown format is supported currently".into());
    }

    let forward_index = load_forward_index(index_dir)?;
    let selection =
        match collect_context_selection(query, from_files, &forward_index, options.max_sections) {
            Ok(selection) => selection,
            Err(ContextSelectionIssue::NoSearchableTerms) => {
                println!("# No searchable terms in query. Try different keywords.");
                return Ok(());
            }
            Err(ContextSelectionIssue::MissingFiles(missing)) => {
                eprintln!(
                    "{}",
                    "Some files were not found in the index (they may be missing or excluded):"
                        .yellow()
                );
                for path in missing {
                    eprintln!("  - {path}");
                }
                return Ok(());
            }
            Err(ContextSelectionIssue::NoIndexedFilesMatched) => {
                println!("# No indexed files matched the provided inputs.");
                return Ok(());
            }
            Err(ContextSelectionIssue::NoRelevantSections(label)) => {
                println!("# No relevant sections found for query: \"{label}\"");
                return Ok(());
            }
        };
    let query_label = selection.query_label;
    let query_for_refiner = selection.query_for_refiner;
    let primary_sections = selection.sections;

    let primary_tokens: usize = primary_sections
        .iter()
        .map(|s| estimate_tokens(&s.content))
        .sum();

    // Phase 2: Cross-reference expansion (if depth > 0)
    let mut all_sections = primary_sections.clone();

    if options.depth > 0 {
        // Calculate xref token budget
        const XREF_TOKEN_FRACTION: f64 = 0.3;
        const XREF_TOKEN_ABS_MAX: usize = 2000;

        let xref_cap =
            ((options.max_tokens as f64 * XREF_TOKEN_FRACTION) as usize).min(XREF_TOKEN_ABS_MAX);
        let remaining_tokens = options.max_tokens.saturating_sub(primary_tokens);
        let xref_token_budget = remaining_tokens.min(xref_cap);

        let primary_docs: HashSet<String> = primary_sections
            .iter()
            .map(|s| s.doc_path.clone())
            .collect();

        if options.use_relations {
            // Graph-aware expansion via persisted relation edges
            let relation_index = load_relation_index(index_dir);
            if !relation_index.edges.is_empty() && xref_token_budget > 0 {
                let xref_sections = resolve_crossrefs_from_relations(
                    &relation_index,
                    &primary_docs,
                    &forward_index,
                    xref_token_budget,
                );
                all_sections.extend(xref_sections);
            }
        } else {
            // Legacy on-the-fly cross-reference expansion
            let adr_index = build_adr_index(&forward_index);
            let crossrefs = collect_crossrefs(&primary_sections, &adr_index);

            if xref_token_budget > 0 && !crossrefs.is_empty() {
                let xref_sections =
                    resolve_crossrefs(&crossrefs, &primary_docs, &forward_index, xref_token_budget);
                all_sections.extend(xref_sections);
            }
        }
    }
    let (all_sections, _) = dedupe_section_matches(all_sections);

    // Phase 3: Extractive refinement (increase signal density)
    let max_tokens_per_section = options.max_tokens / all_sections.len().max(1);
    let refined_sections =
        apply_extractive_refiner(all_sections, &query_for_refiner, max_tokens_per_section);

    // If doc_terms requested, prepend a source summary
    if options.doc_terms > 0 {
        println!("<!-- Source Documents -->");
        let query_terms = if query_for_refiner.is_empty() {
            Vec::new()
        } else {
            parse_query_terms(&query_for_refiner, true)
        };
        let mut seen_docs: HashSet<String> = HashSet::new();

        for section in &refined_sections {
            if seen_docs.contains(&section.section.doc_path) {
                continue;
            }
            seen_docs.insert(section.section.doc_path.clone());

            if let Some(entry) = forward_index.files.get(&section.section.doc_path) {
                let top_terms = get_top_doc_terms(
                    entry,
                    &forward_index.idf_map,
                    &query_terms,
                    options.doc_terms,
                );
                if !top_terms.is_empty() {
                    println!(
                        "<!-- {} : {} -->",
                        section.section.doc_path,
                        top_terms.join(", ")
                    );
                }
            }
        }
        println!();
    }

    // Phase 4: Distill to markdown
    let digest_sections: Vec<SectionMatch> = refined_sections
        .iter()
        .map(|section| section.section.clone())
        .collect();
    let digest = distill_to_markdown(&digest_sections, &query_label, options.max_tokens);

    println!("{digest}");

    Ok(())
}

/// Evaluation command handler - runs retrieval pipeline against test questions
pub(crate) fn cmd_eval(
    questions_path: &Path,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load questions from JSONL file
    let questions_content = fs::read_to_string(questions_path)?;
    let questions: Vec<Question> = questions_content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()?;

    if questions.is_empty() {
        if json {
            println!(
                r#"{{"questions_file": "{}", "total_questions": 0, "error": "No questions found"}}"#,
                questions_path.display()
            );
        } else {
            println!("No questions found in {}", questions_path.display());
        }
        return Ok(());
    }

    // Load index once
    let forward_index = load_forward_index(index_dir)?;

    // Run evaluation for each question
    let mut results = Vec::new();

    for question in &questions {
        // Run assemble internally (capture output as string)
        let primary_sections = search_relevant_sections(&question.q, &forward_index, 20);

        if primary_sections.is_empty() {
            results.push(EvalResult {
                id: question.id,
                question: question.q.clone(),
                hits: 0,
                total: question.expect.len(),
                passed: false,
                tokens: 0,
            });
            continue;
        }

        let primary_tokens: usize = primary_sections
            .iter()
            .map(|s| estimate_tokens(&s.content))
            .sum();

        // Cross-reference expansion
        let mut all_sections = primary_sections.clone();
        let adr_index = build_adr_index(&forward_index);
        let crossrefs = collect_crossrefs(&primary_sections, &adr_index);

        const XREF_TOKEN_FRACTION: f64 = 0.3;
        const XREF_TOKEN_ABS_MAX: usize = 2000;
        let max_tokens: usize = 8000; // Default for eval

        let xref_cap = ((max_tokens as f64 * XREF_TOKEN_FRACTION) as usize).min(XREF_TOKEN_ABS_MAX);
        let remaining_tokens = max_tokens.saturating_sub(primary_tokens);
        let xref_token_budget = remaining_tokens.min(xref_cap);

        if xref_token_budget > 0 && !crossrefs.is_empty() {
            let primary_docs: HashSet<String> = primary_sections
                .iter()
                .map(|s| s.doc_path.clone())
                .collect();

            let xref_sections =
                resolve_crossrefs(&crossrefs, &primary_docs, &forward_index, xref_token_budget);

            all_sections.extend(xref_sections);
        }

        // Extractive refinement
        let max_tokens_per_section = max_tokens / all_sections.len().max(1);
        let refined_sections =
            apply_extractive_refiner(all_sections, &question.q, max_tokens_per_section);

        // Distill to markdown
        let digest_sections: Vec<SectionMatch> = refined_sections
            .iter()
            .map(|section| section.section.clone())
            .collect();
        let digest = distill_to_markdown(&digest_sections, &question.q, max_tokens);

        // Check coverage of expected substrings
        let digest_lower = digest.to_lowercase();
        let hits = question
            .expect
            .iter()
            .filter(|e| digest_lower.contains(&e.to_lowercase()))
            .count();

        let min_hits = question.min_hits.unwrap_or(question.expect.len());
        let passed = hits >= min_hits;
        let tokens = estimate_tokens(&digest);

        results.push(EvalResult {
            id: question.id,
            question: question.q.clone(),
            hits,
            total: question.expect.len(),
            passed,
            tokens,
        });
    }

    // Calculate summary
    let passed_count = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    let pass_rate_pct = passed_count as f64 / total as f64 * 100.0;

    if json {
        let json_results: Vec<EvalQuestionResult> = results
            .iter()
            .map(|r| {
                let expected: Vec<String> = questions
                    .iter()
                    .find(|q| q.id == r.id)
                    .map(|q| q.expect.clone())
                    .unwrap_or_default();
                let found: Vec<String> = expected
                    .iter()
                    .filter(|e| r.question.to_lowercase().contains(&e.to_lowercase()))
                    .cloned()
                    .collect();
                let missing: Vec<String> = expected
                    .iter()
                    .filter(|e| !r.question.to_lowercase().contains(&e.to_lowercase()))
                    .cloned()
                    .collect();
                EvalQuestionResult {
                    question: r.question.clone(),
                    passed: r.passed,
                    expected,
                    found,
                    missing,
                }
            })
            .collect();

        let output = EvalJsonResult {
            questions_file: questions_path.to_string_lossy().to_string(),
            total_questions: total,
            passed: passed_count,
            failed: total - passed_count,
            pass_rate: pass_rate_pct,
            results: json_results,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Print results (human-readable)
    println!("\n{}", "Evaluation Results".cyan().bold());
    println!("{}", "=".repeat(60));
    println!();

    for result in &results {
        let status = if result.passed {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };

        println!("[{}] {}", result.id, result.question.white().bold());
        println!("  - hits: {}/{} {}", result.hits, result.total, status);
        println!("  - size: {} tokens", result.tokens);
        println!();
    }

    // Print summary
    println!("{}", "=".repeat(60));
    println!("{}", "Summary".cyan().bold());
    println!("  Passed: {passed_count}/{total} ({pass_rate_pct:.0}%)");
    println!("  Failed: {}/{}", total - passed_count, total);
    println!();

    if passed_count < total {
        println!("{}", "Failed Questions:".yellow().bold());
        for result in &results {
            if !result.passed {
                println!(
                    "  - [{}] {} (hits: {}/{})",
                    result.id, result.question, result.hits, result.total
                );
            }
        }
        println!();
    }

    Ok(())
}
