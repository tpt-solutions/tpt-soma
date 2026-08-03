use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tpt_soma_harmonize::io::{export_queue_to_csv, import_csv_mappings};
use tpt_soma_harmonize::io::{load_mapping_table, load_queue};
use tpt_soma_harmonize::io::{save_mapping_table, save_queue};
use tpt_soma_harmonize::{Unmapped};

#[derive(Parser)]
#[command(
    name = "review-cli",
    about = "Human-in-the-loop review for unmapped identifiers"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all pending unmapped identifiers
    List {
        /// Path to the review queue JSON file
        #[arg(short, long, default_value = "review_queue.json")]
        queue: PathBuf,
    },
    /// Add an unmapped identifier to the review queue
    Add {
        /// The identifier that couldn't be mapped
        identifier: String,
        /// Source of the identifier (e.g., "dbSNP", "HGNC", "ClinVar")
        source: String,
        /// Path to the review queue JSON file
        #[arg(short, long, default_value = "review_queue.json")]
        queue: PathBuf,
    },
    /// Resolve an unmapped identifier by providing a mapping
    Resolve {
        /// The identifier to resolve
        identifier: String,
        /// The resolved mapping (e.g., rsID or HGNC ID)
        mapping: String,
        /// Path to the review queue JSON file
        #[arg(short, long, default_value = "review_queue.json")]
        queue: PathBuf,
        /// Path to the mapping table JSON file
        #[arg(short, long, default_value = "mapping_table.json")]
        mapping_table: PathBuf,
    },
    /// Export the review queue to a CSV for external review
    Export {
        /// Path to the review queue JSON file
        #[arg(short, long, default_value = "review_queue.json")]
        queue: PathBuf,
        /// Output CSV file path
        #[arg(short, long, default_value = "review_queue.csv")]
        output: PathBuf,
    },
    /// Import resolved mappings from a CSV
    Import {
        /// Input CSV file path
        #[arg(short, long, default_value = "review_queue.csv")]
        input: PathBuf,
        /// Path to the review queue JSON file
        #[arg(short, long, default_value = "review_queue.json")]
        queue: PathBuf,
        /// Path to the mapping table JSON file
        #[arg(short, long, default_value = "mapping_table.json")]
        mapping_table: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List { queue } => {
            let review_queue = load_queue(&queue)?;
            if review_queue.pending.is_empty() {
                println!("No pending unmapped identifiers.");
            } else {
                println!(
                    "Pending unmapped identifiers ({}):",
                    review_queue.pending.len()
                );
                for (i, unmapped) in review_queue.pending.iter().enumerate() {
                    println!(
                        "  {}. {} (source: {})",
                        i + 1,
                        unmapped.identifier,
                        unmapped.source
                    );
                }
            }
        }
        Commands::Add {
            identifier,
            source,
            queue,
        } => {
            let mut review_queue = load_queue(&queue)?;
            review_queue.pending.push(Unmapped { identifier, source });
            save_queue(&queue, &review_queue)?;
            println!("Added to review queue.");
        }
        Commands::Resolve {
            identifier,
            mapping,
            queue,
            mapping_table,
        } => {
            let mut review_queue = load_queue(&queue)?;
            let mut table = load_mapping_table(&mapping_table)?;

            // Find and remove from pending
            let initial_len = review_queue.pending.len();
            review_queue.pending.retain(|u| u.identifier != identifier);

            if review_queue.pending.len() == initial_len {
                println!(
                    "Warning: identifier '{}' not found in review queue",
                    identifier
                );
            }

            // Add to mapping table
            table.insert(identifier.clone(), mapping.clone());

            save_queue(&queue, &review_queue)?;
            save_mapping_table(&mapping_table, &table)?;
            println!("Resolved '{}' -> '{}'", identifier, mapping);
        }
        Commands::Export { queue, output } => {
            let review_queue = load_queue(&queue)?;
            let file = std::fs::File::create(&output)?;
            export_queue_to_csv(&review_queue, file)?;
            println!(
                "Exported {} items to {}",
                review_queue.pending.len(),
                output.display()
            );
        }
        Commands::Import {
            input,
            queue,
            mapping_table,
        } => {
            let mut review_queue = load_queue(&queue)?;
            let mut table = load_mapping_table(&mapping_table)?;
            let file = std::fs::File::open(&input)?;
            let imported = import_csv_mappings(file, &mut table, &mut review_queue)?;

            save_queue(&queue, &review_queue)?;
            save_mapping_table(&mapping_table, &table)?;
            println!("Imported {} resolved mappings", imported);
        }
    }

    Ok(())
}
