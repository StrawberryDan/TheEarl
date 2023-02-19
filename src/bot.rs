use std::collections::HashSet;
use std::process::Command;

use serenity::client::{ClientBuilder, Context, EventHandler};
use serenity::framework::standard::help_commands::plain;
use serenity::framework::standard::macros::{command, group, help};
use serenity::framework::standard::{Args, CommandGroup, CommandResult, HelpOptions};
use serenity::framework::StandardFramework;
use serenity::futures::TryFutureExt;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::prelude::GatewayIntents;
use serenity::{async_trait, Client, Error, FutureExt};
use songbird::driver::{Bitrate, Driver};
use songbird::tracks::Queued;
use songbird::SerenityInit;

#[group]
#[commands(play, join, skip, looping, queue, remove, clear)]
struct General;

pub struct Bot {
    token: String,
    client: Client,
}

impl Bot {
    pub async fn new(token: &str, intents: GatewayIntents) -> Self {
        let framework = StandardFramework::new()
            .configure(|c| c.prefix("~"))
            .group(&GENERAL_GROUP)
            .help(&MY_HELP);

        let client = ClientBuilder::new(token, intents)
            .framework(framework)
            .register_songbird()
            .await
            .expect("Failed to create serenity client!");

        Bot {
            token: token.to_owned(),
            client,
        }
    }

    pub async fn start(&mut self) -> Result<(), Error> {
        self.client.start().await
    }
}

#[command]
#[only_in(guilds)]
#[description = "Requests that the bot join the voice channel the user is currently in."]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let channel_id = guild
        .voice_states
        .get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            msg.reply(ctx, "Not in a voice channel").await.unwrap();

            return Ok(());
        }
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let _handler = manager.join(guild_id, connect_to).await;

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(err) => {
            msg.channel_id
                .say(&ctx.http, "You gotta give a me URL!")
                .await
                .expect("Failed to send message!");
            return Ok(());
        }
    };

    let mut status = msg
        .channel_id
        .say(&ctx.http, "Searching for song")
        .await
        .unwrap();
    let source = songbird::ytdl(&url);

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let Some(channel_id) = guild.voice_states.get(&msg.author.id).and_then(|state| state.channel_id)
    else {
        msg.channel_id.say(&ctx.http, "You must be in a voice channel to summon me smh my head!").await.unwrap();
        return Ok(());
    };

    let manager = songbird::get(ctx).await.unwrap().clone();

    let handler_lock = match manager.get(guild_id) {
        Some(handler) => handler,
        None => manager.join(guild_id, channel_id).await.0,
    };

    match source.await {
        Ok(source) => {
            let mut handler = handler_lock.lock().await;
            handler.set_bitrate(Bitrate::Max);
            handler.enqueue_source(source);
            status
                .edit(&ctx.http, |m| {
                    m.content(format!(
                        "Enqueued song to position {}",
                        handler.queue().len()
                    ))
                })
                .await
                .unwrap();
        }
        Err(e) => {
            status
                .edit(&ctx.http, |m| m.content(format!("Failed to find song!")))
                .await
                .unwrap();
        }
    }

    return Ok(());
}

#[command]
#[only_in(guilds)]
async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await.unwrap().clone();

    let Some(handler_lock) = manager.get(guild_id) else {
        msg.channel_id.say(&ctx.http, "Not applicable").await.unwrap();
        return Ok(());
    };

    let handler = handler_lock.lock().await;

    handler.queue().skip().unwrap();

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await.unwrap().clone();

    let Some(handler_lock) = manager.get(guild_id) else {
        msg.channel_id.say(&ctx.http, "Not applicable").await.unwrap();
        return Ok(());
    };

    let handler = handler_lock.lock().await;

    let queue = handler.queue().current_queue();
    let queue = queue
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "{}. {} ({})",
                i + 1,
                s.metadata().title.clone().unwrap_or("No Title".to_string()),
                s.metadata()
                    .duration
                    .clone()
                    .map(|d| format!("{}:{}", d.as_secs() / 60, d.as_secs() % 60))
                    .unwrap_or("Unknown".to_string())
            )
        })
        .reduce(|a, b| format!("{a}\n{b}"))
        .unwrap_or(String::from("Nothing in Queue!"));

    msg.channel_id.say(&ctx.http, queue).await.unwrap();

    Ok(())
}

#[command]
#[num_args(1)]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let Ok(index) = args.single::<usize>() else {
        msg.channel_id.say(&ctx.http, "You need to specify an index into the queue as a number").await.unwrap();
        return Ok(());
    };

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await.unwrap().clone();

    let Some(handler_lock) = manager.get(guild_id) else {
        msg.channel_id.say(&ctx.http, "Not applicable").await.unwrap();
        return Ok(());
    };

    let handler = handler_lock.lock().await;

    handler.queue().dequeue(index - 1);

    Ok(())
}

#[command]
#[num_args(0)]
async fn clear(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await.unwrap().clone();

    let Some(handler_lock) = manager.get(guild_id) else {
        msg.channel_id.say(&ctx.http, "Not applicable").await.unwrap();
        return Ok(());
    };

    let mut handler = handler_lock.lock().await;

    for i in (0..handler.queue().len()).rev() {
        handler.queue().dequeue(i).unwrap();
    }

    handler.stop();

    Ok(())
}

#[command]
#[only_in(guilds)]
#[aliases("loop")]
#[min_args(0)]
#[max_args(1)]
async fn looping(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let enable = match (args.len()) {
        0 => true,
        1 => match args.single::<String>().unwrap().to_lowercase().as_str() {
            "on" => true,
            "off" => false,
            _ => {
                msg.channel_id
                    .say(&ctx.http, "Invalid argument!")
                    .await
                    .unwrap();
                return Ok(());
            }
        },
        _ => unreachable!(),
    };

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await.unwrap().clone();

    let Some(handler_lock) = manager.get(guild_id) else {
        msg.channel_id.say(&ctx.http, "Not applicable").await.unwrap();
        return Ok(());
    };

    let handler = handler_lock.lock().await;

    if let Some(track) = handler.queue().current() {
        if (enable) {
            match track.enable_loop() {
                Ok(()) => msg
                    .channel_id
                    .say(&ctx.http, "Enabled Looping.")
                    .await
                    .unwrap(),
                Err(_) => msg
                    .channel_id
                    .say(&ctx.http, "Failed to enable looping.")
                    .await
                    .unwrap(),
            };
        } else {
            match track.disable_loop() {
                Ok(()) => msg
                    .channel_id
                    .say(&ctx.http, "Disabled Looping.")
                    .await
                    .unwrap(),
                Err(_) => msg
                    .channel_id
                    .say(&ctx.http, "Failed to disable looping.")
                    .await
                    .unwrap(),
            };
        }
    }

    Ok(())
}

#[help]
async fn my_help(
    ctx: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = plain(ctx, msg, args, help_options, groups, owners).await;
    Ok(())
}
