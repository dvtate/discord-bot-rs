use std::error::Error;
use std::sync::Arc;

// use std::sync::Mutex;
use tokio::sync::Mutex;
use std::time::Duration;

use rss::Channel;

use reqwest;

use serenity::all::MessageBuilder;
use serenity::model::id::ChannelId;

async fn fetch_feed<T: reqwest::IntoUrl>(url: T) -> Result<Channel, Box<dyn Error>> {
    let content = reqwest::get(url)
        .await?
        .bytes()
        .await?;
    let channel = Channel::read_from(&content[..])?;
    Ok(channel)
}

struct RssFeedEntry {
    link: String,
    title: String, 
}

async fn get_new_feed_entries(feed: &mut RssFeedStatus) -> Vec<RssFeedEntry> {
    let mut ret: Vec<RssFeedEntry> = vec![];

    let mut max_ts = feed.last_item_ts;

    match fetch_feed(&feed.url).await {
        Err(why) => {
            println!("failed to fetch rss feed: {why:?}");
            return ret;
        },
        Ok(channel) => {
            for item in channel.items {
                // Rust hasn't implemented this RFC from 6+ years ago
                // https://rust-lang.github.io/rfcs/2497-if-let-chains.html
                if let Some(link) = item.link {
                    if let Some(date) = item.pub_date {
                        match chrono::DateTime::parse_from_rfc2822(&date) {
                            Err(why) => {
                                println!("failed to parse date: {date:?} : {why:?}");
                                return vec![];
                            },
                            Ok(date) => {
                                let ts = date.timestamp();
                                if ts > max_ts {
                                    max_ts = ts;
                                }

                                if ts > feed.last_item_ts {
                                    ret.push(RssFeedEntry {
                                        link: link,
                                        title: item.title.unwrap_or("".to_string()),
                                    });
                                }
                            },
                        };
                        
                    } else {
                        println!("Item missing pub_date");
                    }
                } else {
                    println!("Item missing link");
                }                
            }
        },
    }

    feed.last_item_ts = max_ts;

    ret
}


struct RssFeedStatus {
    url: String,
    last_item_ts: i64,
    channels: Vec<ChannelId>,
}

impl RssFeedStatus {
    async fn fetch(&mut self, http: &Arc<serenity::http::Http>) {
        let items = get_new_feed_entries(self).await;
        let msgs = items.iter().map(
                |item| MessageBuilder::new()
                    .push_bold_line_safe(&item.title)
                    .push_safe(&item.link)
                    .build()
            ).collect::<Vec<String>>();
        
        for chanid in &self.channels {
            for msg in &msgs {
                if let Err(why) = chanid.say(&http, msg).await {
                    println!("failed to send message feed: {why:?}");
                };
            }
        }
    }
}

pub struct RssFeeds {
    feeds: Arc<Mutex<Vec<RssFeedStatus>>>,
}
impl RssFeeds {

    /// Load from db
    async fn load(&self) {

    }

    /// Write to db
    async fn store(&self) {

    }

    /// Add a new rule
    pub async fn subscribe(&self, channel_id: ChannelId, feed_url: String) -> String {
        // self.load().await;



        for f in self.feeds.lock().await.iter_mut() {
            if f.url == feed_url {
                f.channels.push(channel_id);
                self.store().await;
                return "Added subscription".to_string();
            }
        }

        // Check feed is valid
        if let Err(why) = fetch_feed(&feed_url).await {
            println!("Failed to fetch user-requested feed: {feed_url:?} -- {why:?}");
            return "Failed to fetch provided feed. Are you sure you typed it correctly?".to_string();
        }

        self.feeds.lock().await.push(RssFeedStatus {
            url: feed_url,
            last_item_ts: chrono::Utc::now().timestamp(),
            channels: vec![channel_id],
        });
        self.store().await;
        "Added new subscription".to_string()
    }

    async fn cron(&self, http: &Arc<serenity::http::Http>) {
        // self.load().await;
        for f in self.feeds.lock().await.iter_mut() {
            f.fetch(http).await;
        }

        // check again every 5 mins
        tokio::time::sleep(Duration::from_secs(60*5)).await;
        Box::pin(self.cron(http)).await;
    }

    pub async fn start(rssmgr: &Arc<RssFeeds>, http: &Arc<serenity::http::Http>) {
        let httpc = Arc::clone(http);

        let r2 = Arc::clone(rssmgr);
        r2.load().await;

        let _t = tokio::spawn(async move{
            r2.cron(&httpc).await;
        });
    }

    pub fn new() -> RssFeeds {
        RssFeeds { feeds: Arc::new( Mutex::new(  vec![] ) ) }
    }


}

