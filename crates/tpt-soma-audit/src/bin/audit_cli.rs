use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use std::str::FromStr;
use tpt_soma_audit::{AlertSink, AuditLedger, ComplianceReporter, IntegrityAlert, check_and_alert};
use tpt_soma_core::connection::create_pool;

#[derive(Parser)]
#[command(name = "tpt-soma-audit")]
#[command(about = "Audit ledger management for tpt-soma")]
struct Cli {
    /// Database URL
    #[arg(short, long, env = "DATABASE_URL")]
    database_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Verify the hash chain integrity (exit code 1 if it is invalid)
    VerifyChain {
        /// Program to run on failure. Receives AUDIT_ALERT_MESSAGE,
        /// AUDIT_ALERT_EVENT_ID and AUDIT_ALERT_INDEX in its environment.
        #[arg(long)]
        alert_command: Option<String>,
    },
    /// Verify the chain repeatedly and alert on every failure
    Watch {
        /// Seconds between checks
        #[arg(long, default_value = "300")]
        interval_secs: u64,
        /// Program to run on failure (see verify-chain)
        #[arg(long)]
        alert_command: Option<String>,
    },
    /// Show all access to a cohort in a date range
    CohortAccess {
        /// Cohort ID
        #[arg(short, long)]
        cohort: String,
        /// Start date (ISO 8601)
        #[arg(short, long)]
        from: String,
        /// End date (ISO 8601)
        #[arg(short, long)]
        to: String,
    },
    /// Show recent audit events
    Recent {
        /// Number of events to show
        #[arg(short, long, default_value = "50")]
        limit: i64,
    },
    /// Show audit statistics
    Stats,
}

/// Prints alerts to stderr and optionally runs an operator-supplied program.
/// The program is executed directly (no shell) with the alert in its environment.
struct CliAlertSink {
    command: Option<String>,
}

impl AlertSink for CliAlertSink {
    fn alert(&self, alert: &IntegrityAlert) {
        eprintln!("ALERT: {}", alert.message());
        if let Some(cmd) = &self.command {
            let status = std::process::Command::new(cmd)
                .env("AUDIT_ALERT_MESSAGE", alert.message())
                .env("AUDIT_ALERT_EVENT_ID", alert.event_id.to_string())
                .env("AUDIT_ALERT_INDEX", alert.index.to_string())
                .status();
            if let Err(e) = status {
                eprintln!("alert command {cmd:?} failed to start: {e}");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let pool = create_pool(&cli.database_url).await?;
    let ledger = AuditLedger::new(pool.clone());
    let reporter = ComplianceReporter::new(&pool);

    match cli.command {
        Commands::VerifyChain { alert_command } => {
            println!("Verifying audit chain integrity...");
            let sink = CliAlertSink {
                command: alert_command,
            };
            let report = check_and_alert(&ledger, &[&sink]).await?;
            if report.valid {
                println!("✓ Chain is valid");
                if let Some(hash) = report.tail_hash {
                    println!("  Tail hash: {}", hash);
                } else {
                    println!("  Chain is empty");
                }
            } else {
                println!("✗ Chain verification failed!");
                std::process::exit(1);
            }
        }
        Commands::Watch {
            interval_secs,
            alert_command,
        } => {
            let sink = CliAlertSink {
                command: alert_command,
            };
            loop {
                match check_and_alert(&ledger, &[&sink]).await {
                    Ok(r) if r.valid => println!("chain ok ({} events)", r.events_checked),
                    Ok(_) => {}
                    Err(e) => eprintln!("integrity check could not run: {e}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
        }
        Commands::CohortAccess { cohort, from, to } => {
            let from_dt = DateTime::from_str(&from)?;
            let to_dt = DateTime::from_str(&to)?;

            println!("Access to cohort '{}' from {} to {}", cohort, from, to);
            let entries = reporter.access_to_cohort(&cohort, from_dt, to_dt).await?;

            if entries.is_empty() {
                println!("No access events found.");
            } else {
                println!(
                    "{:<36} {:<15} {:<30} {:<10}",
                    "Actor", "Action", "Timestamp", "Outcome"
                );
                println!("{}", "-".repeat(100));
                for entry in entries {
                    println!(
                        "{:<36} {:<15} {:<30} {:<10}",
                        entry.actor,
                        entry.action,
                        entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                        entry.outcome
                    );
                }
            }
        }
        Commands::Recent { limit } => {
            let rows = sqlx::query_as::<_, AuditEventRow>(
                r#"
                SELECT id, actor, resource_class, action, cohort_scope, timestamp, query_fingerprint, outcome
                FROM audit_ledger
                ORDER BY timestamp DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&pool)
            .await?;

            println!("Recent audit events (limit: {}):", limit);
            println!(
                "{:<36} {:<20} {:<15} {:<30} {:<10}",
                "Actor", "Resource", "Action", "Timestamp", "Outcome"
            );
            println!("{}", "-".repeat(120));
            for row in rows {
                println!(
                    "{:<36} {:<20} {:<15} {:<30} {:<10}",
                    row.actor,
                    row.resource_class,
                    row.action,
                    row.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    row.outcome
                );
            }
        }
        Commands::Stats => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_ledger")
                .fetch_one(&pool)
                .await?;

            let by_outcome = sqlx::query_as::<_, OutcomeStat>(
                "SELECT outcome, COUNT(*) as count FROM audit_ledger GROUP BY outcome",
            )
            .fetch_all(&pool)
            .await?;

            let by_resource = sqlx::query_as::<_, ResourceStat>(
                "SELECT resource_class, COUNT(*) as count FROM audit_ledger GROUP BY resource_class ORDER BY count DESC LIMIT 10"
            )
            .fetch_all(&pool)
            .await?;

            let by_actor = sqlx::query_as::<_, ActorStat>(
                "SELECT actor, COUNT(*) as count FROM audit_ledger GROUP BY actor ORDER BY count DESC LIMIT 10"
            )
            .fetch_all(&pool)
            .await?;

            println!("Audit Ledger Statistics");
            println!("=======================");
            println!("Total events: {}", total);
            println!();
            println!("By outcome:");
            for stat in by_outcome {
                println!("  {}: {}", stat.outcome, stat.count);
            }
            println!();
            println!("Top 10 resource classes:");
            for stat in by_resource {
                println!("  {}: {}", stat.resource_class, stat.count);
            }
            println!();
            println!("Top 10 actors:");
            for stat in by_actor {
                println!("  {}: {}", stat.actor, stat.count);
            }
        }
    }

    Ok(())
}

#[derive(sqlx::FromRow)]
struct AuditEventRow {
    actor: String,
    resource_class: String,
    action: String,
    timestamp: DateTime<Utc>,
    outcome: String,
}

#[derive(sqlx::FromRow)]
struct OutcomeStat {
    outcome: String,
    count: i64,
}

#[derive(sqlx::FromRow)]
struct ResourceStat {
    resource_class: String,
    count: i64,
}

#[derive(sqlx::FromRow)]
struct ActorStat {
    actor: String,
    count: i64,
}
