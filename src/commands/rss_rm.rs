use std::sync::Arc;
use crate::bot_rss::RssFeeds;

use serenity::all::{CreateCommandOption, ResolvedValue};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use serenity::model::permissions::Permissions;
use serenity::model::id::ChannelId;

pub async fn run(options: &[ResolvedOption<'_>], channel_id: &ChannelId, rssmgr: &Arc<RssFeeds>) -> String {
    if let Some( ResolvedOption {
        value: ResolvedValue::String(feed_url), ..
    }) = options.first()
    {
        println!("/rssrm {}", feed_url);
        rssmgr.unsubscribe(*channel_id, feed_url.to_string()).await
    } else {
        "Please provide a feed url".to_string()
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new("rssrm")
        .description("Unsubscribe to an rss feed here")
        .default_member_permissions(Permissions::MANAGE_CHANNELS)
        .dm_permission(true)
        .add_option(
            CreateCommandOption::new(serenity::all::CommandOptionType::String, "feed-url", "url to the rss feed")
            .required(true),
        )
}