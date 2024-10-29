use std::{error::Error, sync::Arc, time::Duration};

use chrono::Utc;
use chrono_english::{parse_date_string, Dialect};
use teloxide::{
    macros::BotCommands, prelude::*, types::ReplyParameters,
    utils::command::BotCommands as BCommands,
};
use tokio::join;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "The following commands are supported"
)]
enum Command {
    #[command(description = "shows this text")]
    Help,
    #[command(description = "Reminds you on the solicited date")]
    RemindMe(String),
}

async fn answer(bot: Arc<Bot>, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::RemindMe(s) => match parse_date_string(&s, Utc::now(), Dialect::Uk) {
            Ok(date) => {
                bot.send_message(msg.chat.id, format!("I'll remind you at {}", date))
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
            }
            Err(_) => {
                bot.send_message(msg.chat.id, "Non recognized date format")
                    .await?;
            }
        },
    };

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    log::info!("Iniciando me recorda bot");

    let bot = Arc::new(Bot::from_env());

    let f1 = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => { break; }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            };

            log::warn!("One second");
        }
    });

    let f2 = Command::repl(bot.clone(), answer);

    let _ = join!(f1, f2);

    Ok(())
}
