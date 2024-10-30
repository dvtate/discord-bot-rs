mod bot_rss;
mod commands;

use std::env;
use std::sync::Arc;

use bot_rss::RssFeeds;
use serenity::{async_trait, client};
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::model::application::{Command, Interaction};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;

struct Handler {
    rss_mgr: Arc<RssFeeds>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            println!("Received command interaction: {command:#?}");

            let content = match command.data.name.as_str() {
                "ping" => Some(commands::ping::run(&command.data.options())),
                "rssadd" => Some(commands::rss_add::run(&command.data.options(), &command.channel_id, &self.rss_mgr).await),
                _ => Some("not implemented :(".to_string()),
            };

            if let Some(content) = content {
                let data = CreateInteractionResponseMessage::new().content(content);
                let builder = CreateInteractionResponse::Message(data);
                if let Err(why) = command.create_response(&ctx.http, builder).await {
                    println!("Cannot respond to slash command: {why}");
                }
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        if let Err(e) = Command::create_global_command(&ctx.http, commands::rss_add::register()).await {
            panic!("wtf: {}", e);
        }
        Command::create_global_command(&ctx.http, commands::ping::register()).await;
        
       
        bot_rss::RssFeeds::start(&self.rss_mgr, &ctx.http).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let rssmgr = RssFeeds::new();
    let bot = Handler {
        rss_mgr: Arc::new(rssmgr),
    };
    let mut client = Client::builder(token, GatewayIntents::non_privileged())
        .event_handler(bot)
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform exponential backoff until
    // it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }

}