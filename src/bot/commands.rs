use std::collections::HashSet;

use serenity::client::Context;
use serenity::framework::standard::macros::{command, group, help};
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use songbird::driver::Bitrate;
use songbird::tracks::TrackHandle;

//==================================================================================================
//      Commands
//--------------------------------------------------------------------------------------------------
#[group]
#[commands(play, join, skip, queue, remove, clear, leave)]
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
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    // Say that we are searching.
    let mut status_message = msg
        .channel_id
        .say(&ctx.http, "Searching for song")
        .await
        .unwrap();

    // Get our track.
    let track = match args.single::<String>() {
        Ok(url) => songbird::ytdl(&url).await,
        Err(e) => {
            msg.channel_id
                .say(&ctx.http, "You gotta give a me URL!")
                .await
                .expect(&format!("Failed to send message! {}", e));
            return Ok(());
        }
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
            call.enqueue_source(track);
            status_message
                .edit(&ctx.http, |m| {
                    m.content(format!("Enqueued song to position {}", call.queue().len()))
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
