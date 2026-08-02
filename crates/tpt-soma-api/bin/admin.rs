use clap::{Parser, Subcommand};
use tpt_soma_capability::{registry::DataClassRegistry, token::CapabilityToken};

#[derive(Parser)]
#[command(name = "tpt-soma-admin")]
#[command(about = "Admin CLI for tpt-soma capability tokens and audit queries")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Issue {
        #[arg(short, long)]
        subject: String,
        #[arg(short, long)]
        resource: String,
        #[arg(short, long)]
        action: String,
        #[arg(short, long)]
        cohort: Vec<String>,
    },
    Revoke {
        #[arg(short, long)]
        nonce_hex: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Issue {
            subject,
            resource,
            action,
            cohort,
        } => {
            let _reg = DataClassRegistry::default();
            let token = CapabilityToken {
                subject,
                resource_class: resource,
                cohort_scope: cohort,
                action,
                expiry: 0,
                nonce: Vec::new(),
                signature: Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&token)?);
        }
        Commands::Revoke { nonce_hex } => {
            let nonce = hex::decode(nonce_hex)?;
            println!("revoked nonce: {}", hex::encode(nonce));
        }
    }
    Ok(())
}
