use core::fmt;
use std::{error::Error, sync::Arc, time::Duration};

use chrono::{NaiveDateTime, Utc};
use chrono_english::{parse_date_string, Dialect};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use teloxide::{
    macros::BotCommands,
    prelude::*,
    types::{MessageId, ReplyParameters},
    utils::command::BotCommands as BCommands,
};
use tokio::sync::broadcast::Sender;

const MAX_SLEEP_TIME: u64 = 68719476733;

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

#[derive(Clone, Copy, Debug)]
struct ReplyToMessage {
    message_id: i64,
    reply_to_id: i64,
    chat_id: i64,
    when_send: NaiveDateTime,
}

impl fmt::Display for ReplyToMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

async fn answer(
    bot: Arc<Bot>,
    msg: Message,
    cmd: Command,
    db: Pool<Sqlite>,
    sender: Sender<()>,
) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::RemindMe(s) => match parse_date_string(&s, Utc::now(), Dialect::Uk) {
            Ok(date) => {
                log::info!("Message received and parsed to date {date}");
                bot.send_message(msg.chat.id, format!("I'll remind you at {}", date))
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
                let Ok(_) = sqlx::query!(
                    "INSERT INTO messages (reply_to_id, chat_id, when_send) \
                                              VALUES ($1, $2, $3)",
                    msg.id.0,
                    msg.chat.id.0,
                    date
                )
                .execute(&db)
                .await
                else {
                    bot.send_message(msg.chat.id, "Error saving to the database")
                        .await?;
                    return Ok(());
                };
                sender.send(()).unwrap();
            }
            Err(_) => {
                bot.send_message(msg.chat.id, "Non recognized date format")
                    .await?;
            }
        },
    };

    Ok(())
}

async fn send_message(bot: &Bot, pool: &Pool<Sqlite>, message: ReplyToMessage) {
    log::info!("Sending message {message}");
    bot.send_message(ChatId(message.chat_id), "Reminding you")
        .reply_parameters(ReplyParameters::new(MessageId(
            i32::try_from(message.reply_to_id).unwrap(),
        )))
        .await
        .unwrap();
    sqlx::query!(
        "DELETE FROM messages WHERE message_id = $1",
        message.message_id
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    log::info!("Iniciando me recorda bot");

    let bot = Arc::new(Bot::from_env());
    let bot_clone = bot.clone();

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:./database.sqlite")
        .await?;

    let pool_clone = pool.clone();

    let (sender, mut receiver) = tokio::sync::broadcast::channel(2);

    let _ = tokio::spawn(async move {
        loop {
            match sqlx::query_as!(
                ReplyToMessage,
                "SELECT message_id, reply_to_id, chat_id, when_send FROM messages ORDER BY when_send LIMIT 1"
            )
                .fetch_one(&pool_clone)
                .await {
                    Ok(next_message) =>
                        if next_message.when_send < Utc::now().naive_utc() {
                            send_message(&bot_clone, &pool_clone, next_message).await;
                        } else {
                            let sleep_time = std::cmp::min(
                                Duration::from_millis(MAX_SLEEP_TIME),
                                (next_message.when_send - Utc::now().naive_utc())
                                    .to_std()
                                    .unwrap()
                            );
                            tokio::select! {
                                _ = tokio::signal::ctrl_c() => {
                                    log::info!("Stopping worker");
                                    break;
                                }
                                _ = tokio::time::sleep(sleep_time) => {}
                                _ = receiver.recv() => {}
                            }
                        },
                    Err(_) => tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            log::info!("Stopping worker");
                            break;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(MAX_SLEEP_TIME)) => {}
                        _ = receiver.recv() => {}
                    }
                }
        }
    });

    Command::repl(bot.clone(), move |bot, msg, cmd| {
        answer(bot, msg, cmd, pool.clone(), sender.clone())
    })
    .await;

    Ok(())
}
