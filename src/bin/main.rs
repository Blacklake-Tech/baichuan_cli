use baichuan_cli::{make_baichuan_request, Model};
use clap::Parser;
use env_logger::Builder;
use log::{debug, error, info, LevelFilter};
use rustyline::{error::ReadlineError, DefaultEditor, Result};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env)]
    api_key: String,
    #[arg(long, env)]
    secret_key: String,
    #[arg(short, long, value_enum, default_value_t = Model::Baichuan2_53B)]
    model: Model,
    #[arg(long, default_value_t = LevelFilter::Info)]
    log_level: LevelFilter,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    Builder::new().filter_level(args.log_level).init();

    let mut rl = DefaultEditor::new()?;
    if rl.load_history(".bc_cli_history").is_err() {
        debug!("No previous history loaded.");
    }
    loop {
        let readline = rl.readline("❯ ");
        match readline {
            Ok(line) => {
                let r =
                    make_baichuan_request(&args.api_key, &args.secret_key, args.model, vec![line])
                        .await;
                match r {
                    Ok(resp) => {
                        if let Some(data) = resp.data {
                            data.messages.iter().for_each(|message| {
                                println!("[{}]: {}", message.role, message.content)
                            })
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to request API: {}", e)
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                info!("👋");
                break;
            }
            Err(ReadlineError::Eof) => {
                info!("👋");
                break;
            }
            Err(err) => {
                error!("Error: {:?}", err);
                break;
            }
        }
    }
    if rl.save_history(".bc_cli_history").is_err() {
        error!("Could not save history.");
    }
    Ok(())
}
