use std::io::Write;
use std::io::stdout;

use clap::{Parser, Subcommand};
use hypersdk::{
    Address,
    hypercore::{self, types::UserBalance},
};

#[derive(Parser)]
#[command(author, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    // with_url: Url,
}

#[derive(Subcommand)]
enum Commands {
    /// Gather spot balances for a user.
    SpotBalances { user: Address },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let core = hypercore::mainnet();
    match args.command {
        Commands::SpotBalances { user } => {
            let balances = core.user_balances(user).await?;
            print_balances(balances);
        }
    }

    Ok(())
}

fn print_balances(balances: Vec<UserBalance>) {
    let mut writer = tabwriter::TabWriter::new(stdout());

    writeln!(&mut writer, "coin\thold\ttotal");
    for balance in balances {
        writeln!(
            &mut writer,
            "{}\t{}\t{}",
            balance.coin, balance.hold, balance.total
        );
    }

    let _ = writer.flush();
}
