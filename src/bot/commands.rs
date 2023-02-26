use std::collections::HashSet;

use serenity::client::Context;
use serenity::framework::standard::macros::{command, group, help};
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use songbird::driver::Bitrate;
use songbird::input::Input;
use songbird::tracks::TrackHandle;

//==================================================================================================
//      Commands
//--------------------------------------------------------------------------------------------------
#[group]
#[commands(play, join, looping, skip, queue, remove, clear, leave)]
struct Commands;

#[command]
#[only_in(guilds)]
#[description = "Requests that the bot join the voice channel the user is currently in."]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let Some(channel) = guild
        .voice_states
        .get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id)
    else {
        return Ok(())
    };

    let manager = songbird::get(ctx).await.unwrap().clone();

    let (_, result) = manager.join(guild_id, channel).await;
    if let Err(e) = result {
        eprintln!("{}", e);
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    clear(ctx, msg, Args::new("", &[]));

    let guild = msg.guild(&ctx.cache).unwrap().id;
    let manager = songbird::get(&ctx).await.unwrap().clone();
    manager.leave(guild).await.unwrap();

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    // Say that we are searching.
    let mut status_message = msg
        .channel_id
        .say(&ctx.http, "Searching for song")
        .await
        .unwrap();

    let argument = args.rest().to_string();

    // Get our track.
    let track = if is_url(&argument) {
        songbird::input::Restartable::ytdl(argument, true).await
    } else {
        songbird::input::Restartable::ytdl_search(argument, true).await
    };

    // Get channel that the user is in.
    let guild = msg.guild(&ctx.cache).unwrap();
    let Some(channel) = guild.voice_states
        .get(&msg.author.id)
        .and_then(|state| state.channel_id)
    else { return Ok(()); };

    // Join the call
    let manager = songbird::get(ctx).await.unwrap().clone();
    let call = match manager.join(guild.id, channel).await {
        // Successful join.
        (call, Ok(())) => {
            call.lock().await.set_bitrate(Bitrate::Max);
            call
        }
        // Failed to join.
        (_, Err(e)) => {
            eprintln!("Could not join call! {}", e);
            return Ok(());
        }
    };

    // Enqueue Track.
    match track {
        Ok(track) => {
            let mut call = call.lock().await;
            let track: Input = track.into();
            let track_name = track
                .metadata
                .source_url
                .clone()
                .unwrap_or("No URL".to_string());
            call.enqueue_source(track);
            status_message
                .edit(&ctx.http, |m| {
                    m.content(format!(
                        "Enqueued song to position {}\n{}",
                        call.queue().len(),
                        track_name
                    ))
                })
                .await
                .unwrap();
        }
        Err(e) => {
            status_message
                .edit(&ctx.http, |m| {
                    m.content(format!("Failed to find song! {}", e))
                })
                .await
                .unwrap();
        }
    }

    return Ok(());
}

fn is_url(string: &str) -> bool {
    string.starts_with("http://") || string.starts_with("https://")
}

#[command]
#[only_in(guilds)]
#[aliases("loop")]
#[min_args(0)]
#[max_args(1)]
async fn looping(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let enable = match args.len() {
        0 => true,
        1 => match args.single::<String>().unwrap().to_lowercase().as_str() {
            "on" => true,
            "off" => false,
            _ => return Ok(()),
        },
        _ => unreachable!(),
    };

    let guild = msg.guild(&ctx.cache).unwrap();

    let songbird = songbird::get(ctx).await.unwrap().clone();

    let call = songbird.get_or_insert(guild.id);
    let call = call.lock().await;

    if let Some(track) = call.queue().current() {
        if enable {
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

#[command]
#[only_in(guilds)]
async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();

    let songbird = songbird::get(ctx).await.unwrap().clone();
    let Some(call) = songbird.get(guild.id) else {
        return Ok(());
    };

    call.lock().await.queue().skip().unwrap();

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();

    let songbird = songbird::get(ctx).await.unwrap().clone();
    let Some(call) = songbird.get(guild.id) else {
        return Ok(());
    };
    let call = call.lock().await;

    let queue = call.queue().current_queue();
    let queue_string = queue
        .into_iter()
        .enumerate()
        .map(|(i, s)| track_as_queue_string(i, &s))
        .reduce(|a, b| format!("{a}\n{b}"))
        .unwrap_or("Nothing in Queue!".to_string());
    msg.channel_id.say(&ctx.http, queue_string).await.unwrap();

    Ok(())
}

fn track_as_queue_string(index: usize, track: &TrackHandle) -> String {
    format!(
        "{}. {} ({})",
        index + 1,
        track
            .metadata()
            .title
            .clone()
            .unwrap_or("No Title".to_string()),
        track
            .metadata()
            .duration
            .clone()
            .map(|d| format!("{}:{}", d.as_secs() / 60, d.as_secs() % 60))
            .unwrap_or("Unknown".to_string())
    )
}

#[command]
#[num_args(1)]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let Ok(index) = args.single::<usize>() else {
        return Ok(());
    };

    let guild = msg.guild(&ctx.cache).unwrap();
    let songbird = songbird::get(ctx).await.unwrap().clone();
    let Some(call) = songbird.get(guild.id) else {
        return Ok(());
    };
    let call = call.lock().await;

    call.queue().dequeue(index - 1);

    Ok(())
}

#[command]
async fn clear(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();

    let songbird = songbird::get(ctx).await.unwrap().clone();

    let Some(call) = songbird.get(guild.id) else {
        return Ok(());
    };

    let mut call = call.lock().await;
    while call.queue().len() > 0 {
        call.queue().dequeue(0);
    }

    call.stop();

    Ok(())
}

#[help]
async fn my_help(
    ctx: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static serenity::framework::standard::HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = serenity::framework::standard::help_commands::plain(
        ctx,
        msg,
        args,
        help_options,
        groups,
        owners,
    )
    .await;
    Ok(())
}
