use crate::bot::Bot;
use serenity::prelude::GatewayIntents;

pub mod bot;

#[tokio::main]
async fn main() {
    let token = std::env::var("DISCORD_TOKEN")
        .expect("Expected a value from environment variable DISCORD_TOKEN");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;

    let mut bot = Bot::new(&token, intents).await;
    if let Err(why) = bot.start().await {
        panic!("Failed to start bot with reason {why}");
    }
}
