use std::env;
use std::fs;

use serenity::all::MessageBuilder;
use serenity::model::id::ChannelId;



struct AnnounceRule {
    channel_id: ChannelId,
    msg_template: String,
}


fn template(msg: String, guild_name: String, user: String, guild_size: u64) {
    let mb = MessageBuilder::new();
}