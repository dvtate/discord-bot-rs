use std::sync::Arc;
use crate::bot_rss::RssFeeds;

use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use serenity::model::id::ChannelId;

pub async fn run(_options: &[ResolvedOption<'_>], channel_id: &ChannelId, rssmgr: &Arc<RssFeeds>) -> String {
    println!("/rsssubs {}", channel_id);
    rssmgr.channel_subs(*channel_id).await
}

pub fn register() -> CreateCommand {
    CreateCommand::new("rsssubs").description("Show all the RSS feeds this channel is subscribed to")
}