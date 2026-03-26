// Suppress errors about lints that exist in one clippy version but are
// renamed/removed in another (e.g. match_on_vec_items removed in 1.94+).
#![allow(renamed_and_removed_lints)]
// Pedantic lint config: enable pedantic, then allow categories that are
// not worth fixing across a 13K-line single-file CLI.
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::items_after_statements,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::wildcard_imports,
    // These are on already-lowercased strings; using Path::extension()
    // would be less readable for our use case.
    clippy::case_sensitive_file_extension_comparisons,
    clippy::fn_params_excessive_bools,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::unnecessary_wraps,
    clippy::match_on_vec_items,
    clippy::implicit_clone,
    clippy::ref_option
)]

mod assemble;
mod cli;
mod commands_audit;
mod commands_graph;
mod commands_links;
mod commands_query;
mod commands_text;
mod config;
mod index;
mod mcp;
mod search;
mod types;
mod util;
use cli::*;
use commands_audit::*;
use commands_graph::*;
use commands_links::*;
use commands_query::*;
use commands_text::*;
use config::*;
use index::*;
use mcp::*;
pub use types::*;

use clap::Parser;
use colored::Colorize;
use std::path::PathBuf;

// Re-exports consumed by `mod tests { use super::*; }`.
#[allow(unused_imports)]
use assemble::*;
#[allow(unused_imports)]
use globset::Glob;
#[allow(unused_imports)]
use regex::Regex;
#[allow(unused_imports)]
use search::*;
#[allow(unused_imports)]
use serde::Serialize;
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[allow(unused_imports)]
use std::fs;
#[allow(unused_imports)]
use std::io::{self, Write};
#[allow(unused_imports)]
use std::path::Path;
#[allow(unused_imports)]
use std::time::Instant;
#[allow(unused_imports)]
use util::*;

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {}", "error".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Handle SIGPIPE / broken pipe panics gracefully (e.g., when piping into `head`).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{info}");
        if msg.contains("Broken pipe (os error 32)") {
            // Treat broken pipe as a normal early exit with success.
            std::process::exit(0);
        }
        default_hook(info);
    }));

    let cli = Cli::parse();
    let config = load_config(&cli.config, cli.quiet);

    let result = match cli.command {
        Commands::Check {
            links,
            dupes: _,
            taxonomy,
            stale,
            ci,
            fail_on,
            index,
            policy,
            stale_days,
        } => {
            let index_path = resolve_index_path(index, cli.profile.as_deref(), &config);

            let mut combined = CombinedCheckResult::default();

            // Run link checks if requested
            if links {
                let include_summary = true;
                let external_paths: Vec<String> = config
                    .as_ref()
                    .and_then(|c| c.external.as_ref())
                    .map(|e| e.repos.iter().map(|r| r.path.clone()).collect())
                    .unwrap_or_default();
                let link_result =
                    run_link_check(&index_path, None, include_summary, false, &external_paths)?;
                combined.links = Some(link_result);
            }

            // Run policy checks if requested
            if taxonomy {
                let policy_path = match policy {
                    Some(p) => p,
                    None => PathBuf::from(".yore-policy.yaml"),
                };
                let policy_result = run_policy_check(&index_path, &policy_path)?;
                combined.policy = Some(policy_result);
            }

            // Run staleness checks if requested
            if stale {
                let stale_result = run_stale_check(&index_path, stale_days, 0)?;
                combined.stale = Some(stale_result);
            }

            // For now, `check` always prints JSON.
            let json_str = serde_json::to_string_pretty(&combined)?;
            println!("{json_str}");

            // CI/fail-on logic: allow both link kinds and policy severities.
            if ci && !fail_on.is_empty() {
                let mut should_fail = false;

                // Link-based failure conditions (existing behavior)
                if links {
                    if let Some(link_result) = &combined.links {
                        if let Some(summary) = &link_result.summary {
                            for key in &fail_on {
                                if let Some(kind) = summary.by_kind.iter().find(|k| &k.kind == key)
                                {
                                    if kind.count > 0 {
                                        should_fail = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                // Policy-based failure conditions: keyed by severity.
                // Supported keys:
                //   - "policy_error"  – fail if any violation has severity "error"
                //   - "policy_warn"   – fail if any violation has severity "warn" / "warning"
                if taxonomy {
                    if let Some(policy_result) = &combined.policy {
                        let fail_on_error = fail_on.iter().any(|k| k == "policy_error");
                        let fail_on_warn = fail_on
                            .iter()
                            .any(|k| k == "policy_warn" || k == "policy_warning");

                        if fail_on_error || fail_on_warn {
                            for v in &policy_result.violations {
                                let sev = v.severity.as_str();
                                if (fail_on_error && sev == "error")
                                    || (fail_on_warn && (sev == "warn" || sev == "warning"))
                                {
                                    should_fail = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if should_fail {
                    std::process::exit(1);
                }
            }

            Ok(())
        }
        Commands::Health {
            file,
            all,
            index,
            max_lines,
            max_part_sections,
            max_completed_lines,
            max_changelog_entries,
            json,
        } => cmd_health(
            file.as_deref(),
            all,
            &index,
            &HealthOptions {
                max_lines,
                max_part_sections,
                max_completed_lines,
                max_changelog_entries,
            },
            json,
        ),
        Commands::Build {
            path,
            output,
            types,
            exclude,
            json,
            track_renames,
        } => {
            let (path, output, types, roots) =
                resolve_build_params(path, output, types, cli.profile.as_deref(), &config);
            cmd_build(
                &path,
                &output,
                &types,
                &exclude,
                cli.quiet,
                roots.as_deref(),
                json,
                track_renames,
            )
        }
        Commands::Query {
            terms,
            query,
            limit,
            files_only,
            json,
            doc_terms,
            explain,
            no_stopwords,
            phrase,
            index,
        } => {
            let query_text = query.unwrap_or_else(|| terms.join(" "));
            let options = QueryOptions {
                limit,
                files_only,
                json,
                doc_terms,
                explain,
                require_phrases: phrase,
                filter_stopwords: !no_stopwords,
            };
            cmd_query(&query_text, &index, &options)
        }
        Commands::Similar {
            file,
            limit,
            threshold,
            json,
            doc_terms,
            index,
        } => cmd_similar(&file, limit, threshold, json, doc_terms, &index),
        Commands::Dupes {
            threshold,
            group,
            json,
            index,
        } => cmd_dupes(threshold, group, json, &index),
        Commands::DupesSections {
            threshold,
            min_files,
            json,
            index,
        } => cmd_dupes_sections(threshold, min_files, json, &index),
        Commands::Diff {
            file1,
            file2,
            index,
            json,
        } => cmd_diff(&file1, &file2, &index, json),
        Commands::Stats {
            top_keywords,
            index,
            json,
        } => cmd_stats(top_keywords, &index, json),
        Commands::Repl { index } => cmd_repl(&index),
        Commands::Assemble {
            query,
            max_tokens,
            max_sections,
            depth,
            format,
            doc_terms,
            from_files,
            use_relations,
            index,
        } => cmd_assemble(
            &query.join(" "),
            &from_files,
            &AssembleOptions {
                max_tokens,
                max_sections,
                depth,
                format,
                doc_terms,
                use_relations,
            },
            &index,
        ),
        Commands::Mcp { command } => match command {
            McpCommands::SearchContext {
                query,
                max_results,
                max_tokens,
                max_bytes,
                from_files,
                index,
            } => cmd_mcp_search_context(
                &query.join(" "),
                &from_files,
                &index,
                McpSearchOptions {
                    max_results,
                    max_tokens,
                    max_bytes,
                },
            ),
            McpCommands::FetchContext {
                handle,
                max_tokens,
                max_bytes,
                index,
            } => cmd_mcp_fetch_context(
                &handle,
                &index,
                McpFetchOptions {
                    max_tokens,
                    max_bytes,
                },
            ),
            McpCommands::Serve { index } => cmd_mcp_serve(&index),
        },
        Commands::Eval {
            questions,
            index,
            json,
        } => cmd_eval(&questions, &index, json),
        Commands::Vocabulary {
            index,
            limit,
            format,
            json,
            stopwords,
            include_stemming,
            no_default_stopwords,
            common_terms,
        } => cmd_vocabulary(
            &index,
            limit,
            &format,
            json,
            VocabularyOptions {
                stopwords: stopwords.as_deref(),
                include_stemming,
                no_default_stopwords,
                common_terms,
            },
        ),
        Commands::CheckLinks {
            index,
            json,
            root,
            summary,
            summary_only,
        } => {
            let index_path = resolve_index_path(index, cli.profile.as_deref(), &config);
            let external_paths: Vec<String> = config
                .as_ref()
                .and_then(|c| c.external.as_ref())
                .map(|e| e.repos.iter().map(|r| r.path.clone()).collect())
                .unwrap_or_default();
            cmd_check_links(
                &index_path,
                json,
                root.as_deref(),
                summary,
                summary_only,
                &external_paths,
            )
        }
        Commands::Backlinks { file, index, json } => cmd_backlinks(&file, &index, json),
        Commands::Orphans {
            index,
            json,
            exclude,
        } => cmd_orphans(&index, json, &exclude),
        Commands::Canonicality {
            index,
            json,
            threshold,
        } => cmd_canonicality(&index, json, threshold),
        Commands::CanonicalOrphans {
            index,
            json,
            threshold,
        } => cmd_canonical_orphans(&index, threshold, json),
        Commands::ExportGraph { format, index } => cmd_export_graph(&index, &format),
        Commands::Paths {
            source,
            depth,
            kind,
            json,
            index,
        } => cmd_paths(&source, depth, kind.as_deref(), json, &index),
        Commands::SuggestConsolidation {
            threshold,
            json,
            index,
        } => cmd_suggest_consolidation(&index, threshold, json),
        Commands::Policy {
            config,
            index,
            json,
        } => cmd_policy(&config, &index, json),
        Commands::FixLinks {
            index,
            dry_run,
            apply,
            propose,
            apply_decisions,
            json,
            use_git_history,
        } => cmd_fix_links(
            &index,
            dry_run,
            apply,
            propose,
            apply_decisions,
            json,
            use_git_history,
        ),
        Commands::FixReferences {
            mapping,
            index,
            dry_run,
            apply,
            json,
        } => cmd_fix_references(&index, &mapping, dry_run, apply, json),
        Commands::Mv {
            from,
            to,
            index,
            update_refs,
            dry_run,
            json,
        } => cmd_mv(&from, &to, &index, update_refs, dry_run, json),
        Commands::Stale {
            index,
            days,
            min_inlinks,
            json,
        } => cmd_stale(&index, days, min_inlinks, json),
    };
    result
}

#[cfg(test)]
#[path = "tests_main.rs"]
mod tests;
