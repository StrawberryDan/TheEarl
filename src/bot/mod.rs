use serenity::client::{ClientBuilder, Context, EventHandler};
use serenity::framework::StandardFramework;
use serenity::model::channel::ChannelType;
use serenity::model::prelude::VoiceState;
use serenity::prelude::GatewayIntents;
use serenity::{async_trait, Client, Error};
use songbird::SerenityInit;

mod commands;

//==================================================================================================
//      Handler
//--------------------------------------------------------------------------------------------------
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn voice_state_update(&self, ctx: Context, _: Option<VoiceState>, new: VoiceState) {
        let channels = new.guild_id.unwrap().channels(&ctx.http).await.unwrap();
        let channels: Vec<_> = channels
            .values()
            .filter(|c| c.kind == ChannelType::Voice)
            .collect();
        for channel in channels.into_iter() {
            let members = channel.members(&ctx.cache).await.unwrap();
            if members
                .iter()
                .any(|u| u.user.id == ctx.cache.current_user().id)
            {
                if members.len() == 1 {
                    let manager = songbird::get(&ctx).await.unwrap();

                    manager.leave(new.guild_id.unwrap()).await.unwrap();
                }
            }
        }
    }
}

//==================================================================================================
//      Bot
//--------------------------------------------------------------------------------------------------
pub struct Bot {
    // token: String,
    client: Client,
}

impl Bot {
    pub async fn new(token: &str, intents: GatewayIntents) -> Self {
        let framework = StandardFramework::new()
            .configure(|c| c.prefix("~"))
            .group(&commands::COMMANDS_GROUP)
            .help(&commands::MY_HELP);

        let client = ClientBuilder::new(token, intents)
            .framework(framework)
            .event_handler(Handler)
            .register_songbird()
            .await
            .expect("Failed to create serenity client!");

        Bot {
            // token: token.to_owned(),
            client,
        }
    }

    pub async fn start(&mut self) -> Result<(), Error> {
        self.client.start().await
    }
}
