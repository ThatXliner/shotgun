//! Wires the CLI subcommands to the spec parser, diff engine, mapping I/O,
//! and proxy server.

use crate::diff::build_mapping_file;
use crate::mapping::merge::merge_mapping_files;
use crate::mapping::reader::{read_mapping_file, MappingFormat};
use crate::mapping::writer::write_mapping_file;
use crate::spec::parser::parse_spec_file;
use anyhow::Context;
use chrono::Utc;

pub fn run_init(source: &str, target: &str, output: &str, format: &str) -> anyhow::Result<()> {
    let source_spec = parse_spec_file(source).context("parsing source spec")?;
    let target_spec = parse_spec_file(target).context("parsing target spec")?;

    let generated_at = Utc::now().to_rfc3339();
    let (mapping, summary) = build_mapping_file(&source_spec, &target_spec, source, target, &generated_at);

    let fmt = MappingFormat::parse(format);
    write_mapping_file(&mapping, output, fmt)?;

    println!(
        "Mapped {}/{} endpoints automatically. {} unmapped (no match), {} unmapped (path matched but method differs).",
        summary.mapped, summary.total_source_endpoints, summary.unmatched, summary.method_mismatches
    );
    println!("Wrote {output}");
    Ok(())
}

pub fn run_sync(source: &str, target: &str, mappings: &str) -> anyhow::Result<()> {
    let existing = read_mapping_file(mappings).context("reading existing mappings")?;

    let source_spec = parse_spec_file(source).context("parsing source spec")?;
    let target_spec = parse_spec_file(target).context("parsing target spec")?;
    let generated_at = Utc::now().to_rfc3339();
    let (proposed, _summary) = build_mapping_file(&source_spec, &target_spec, source, target, &generated_at);

    let (merged, report) = merge_mapping_files(existing, proposed);
    let fmt = MappingFormat::from_path(mappings);
    write_mapping_file(&merged, mappings, fmt)?;

    println!(
        "Synced {mappings}: {} added, {} updated, {} kept (edited), {} flagged as removed.",
        report.added, report.updated, report.kept_edited, report.removed_flagged
    );
    Ok(())
}

pub fn run_validate(mappings: &str) -> anyhow::Result<()> {
    let mf = read_mapping_file(mappings).context("reading mappings")?;
    let (mapped, total) = mf.coverage();

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for ep in &mf.endpoints {
        if ep.target.is_empty() {
            warnings.push(format!("{}: no target configured", ep.source));
            continue;
        }
        if crate::mapping::types::EndpointMapping::method_and_path(&ep.target).is_none() {
            errors.push(format!("{}: target '{}' is not a valid 'METHOD /path'", ep.source, ep.target));
        }
        if !ep.response.type_conflicts.is_empty() || !ep.request.type_conflicts.is_empty() {
            warnings.push(format!("{}: has unresolved type conflicts, needs review", ep.source));
        }
        for nested in &ep.response.nested {
            if mf.schema_by_name(&nested.schema_map).is_none() {
                errors.push(format!(
                    "{}: response.nested references unknown schema '{}'",
                    ep.source, nested.schema_map
                ));
            }
        }
    }

    println!("Coverage: {mapped}/{total} endpoints mapped ({}%)", if total == 0 { 0 } else { mapped * 100 / total });
    println!("Schemas defined: {}", mf.schemas.len());
    println!();

    if errors.is_empty() && warnings.is_empty() {
        println!("No errors or warnings.");
    } else {
        for e in &errors {
            println!("ERROR: {e}");
        }
        for w in &warnings {
            println!("WARN:  {w}");
        }
    }

    if !errors.is_empty() {
        anyhow::bail!("{} error(s) found", errors.len());
    }
    Ok(())
}
