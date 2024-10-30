use core::fmt;
use std::{error::Error, time::Duration};
use std::sync::Arc;
use chrono::{NaiveDateTime, TimeDelta, Utc};
use chrono_english::{parse_date_string, Dialect};
use futures_util::StreamExt;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use teloxide::{
    macros::BotCommands,
    prelude::*,
    types::{MessageId, ReplyParameters},
    utils::command::BotCommands as BCommands,
};
use tokio::sync::broadcast::Sender;

const MAX_SLEEP_TIME: Duration = Duration::from_millis(68719476733);

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
    message_added: Sender<()>,
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
                message_added.send(()).unwrap();
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

    let bot = Bot::from_env();

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:./database.sqlite")
        .await?;

    let (message_added, mut wake_up_consumer) = tokio::sync::broadcast::channel(2);

    let consume_messages = tokio::spawn({
        let bot = bot.clone();
        let db = db.clone();
        async move {
            loop {
                let mut messages = sqlx::query_as!(
                ReplyToMessage,
                "SELECT message_id, reply_to_id, chat_id, when_send FROM messages ORDER BY when_send"
            ).fetch(&db);

                let now = Utc::now();
                let mut sleep_for: Duration = MAX_SLEEP_TIME;

                while let Some(message) = messages.next().await {
                    let message = match message {
                        Ok(message) => message,
                        Err(error) => {
                            log::error!("Failed to fetch message, skipping: {error:?}");
                            continue;
                        }
                    };

                    let should_send_in = message.when_send.and_utc() - now;
                    if should_send_in > TimeDelta::zero() {
                        sleep_for = should_send_in.to_std()
                            .expect("Can only sleep for a positive amount of time");
                        break;
                    }

                    send_message(&bot, &db, message).await;
                }

                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    _ = tokio::time::sleep(sleep_for) => {},
                    _ = wake_up_consumer.recv() => {}
                }
            }

            log::info!("Stopping worker");
        }
    });

    Command::repl(bot.clone(), move |bot, msg, cmd| {
        answer(bot, msg, cmd, db.clone(), message_added.clone())
    })
    .await;

    consume_messages.await?;

    Ok(())
}
