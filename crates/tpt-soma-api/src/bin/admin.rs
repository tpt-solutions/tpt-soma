use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use tpt_soma_capability::{
    registry::DataClassRegistry,
    revocation::RevocationList,
    signing::{LocalSigningBackend, SigningBackend},
    token::CapabilityToken,
};

#[derive(Parser)]
#[command(name = "tpt-soma-admin")]
#[command(about = "Admin CLI for tpt-soma API")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new root signing key pair
    GenKey {
        /// Output directory for key files
        #[arg(short, long, default_value = "./dev-keys")]
        output: PathBuf,
    },
    /// Issue a new capability token
    Issue {
        /// Subject (researcher identifier)
        #[arg(short, long)]
        subject: String,
        /// Resource class (e.g., genomic_variant, transcriptomic_scrna)
        #[arg(short, long)]
        resource_class: String,
        /// Cohort scope (comma-separated)
        #[arg(short, long)]
        cohort: String,
        /// Action (e.g., read, write, admin)
        #[arg(short, long, default_value = "read")]
        action: String,
        /// Expiry in seconds from now
        #[arg(short, long, default_value = "3600")]
        expiry: u64,
        /// Path to root signing key
        #[arg(short, long, default_value = "./dev-keys/signing_key.bin")]
        key: PathBuf,
        /// Output file for token (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List registered data classes
    ListClasses,
    /// Revoke a token by nonce
    Revoke {
        /// Nonce of the token to revoke (hex encoded)
        #[arg(short, long)]
        nonce: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenKey { output } => {
            fs::create_dir_all(&output)?;
            let backend = LocalSigningBackend::generate();
            let verifying_key = backend.verifying_key();

            fs::write(output.join("signing_key.bin"), backend.to_bytes())?;
            fs::write(output.join("verifying_key.bin"), verifying_key.to_bytes())?;

            println!("Generated key pair in {}", output.display());
            println!("Signing key: {}", output.join("signing_key.bin").display());
            println!(
                "Verifying key: {}",
                output.join("verifying_key.bin").display()
            );
        }
        Commands::Issue {
            subject,
            resource_class,
            cohort,
            action,
            expiry,
            key,
            output,
        } => {
            let key_bytes = fs::read(key)?;
            let mut key_arr = [0u8; 32];
            key_arr.copy_from_slice(&key_bytes);
            let backend = LocalSigningBackend::from_bytes(&key_arr);

            let mut registry = DataClassRegistry::default();
            registry.seed_phase0();
            registry.seed_phase2();
            registry.seed_phase3();
            registry.seed_phase4();

            if registry.get(&resource_class).is_none() {
                eprintln!(
                    "Warning: resource class '{}' not in registry",
                    resource_class
                );
            }

            let cohort_scope: Vec<String> =
                cohort.split(',').map(|s| s.trim().to_string()).collect();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            let mut token = CapabilityToken {
                subject,
                resource_class,
                cohort_scope,
                action,
                expiry: now + expiry,
                nonce: rand::random::<[u8; 32]>().to_vec(),
                signature: Vec::new(),
                graph_scope: None,
            };

            token = CapabilityToken::sign(&backend, token);

            let token_json = serde_json::to_string_pretty(&token)?;

            if let Some(output_path) = output {
                fs::write(&output_path, &token_json)?;
                println!("Token written to {}", output_path.display());
            } else {
                println!("{}", token_json);
            }
        }
        Commands::ListClasses => {
            let mut registry = DataClassRegistry::default();
            registry.seed_phase0();
            registry.seed_phase2();
            registry.seed_phase3();
            registry.seed_phase4();

            println!("Registered data classes:");
            for class in registry.list() {
                println!(
                    "  {} - {} ({:?})",
                    class.id, class.description, class.sensitivity
                );
            }
        }
        Commands::Revoke { nonce } => {
            let nonce_bytes = hex::decode(nonce)?;
            let rt = tokio::runtime::Runtime::new()?;
            let revocation_list = RevocationList::new();
            rt.block_on(async {
                revocation_list.revoke(nonce_bytes).await;
                println!("Token revoked");
            });
        }
    }

    Ok(())
}
