use std::error::Error;
use std::sync::Arc;
use std::env;
use std::fs;
use std::time::Duration;

use std::mem;

use bytes::Buf;

use json::array;
use json::object;

use tokio::sync::Mutex;

use feed_rs;

use reqwest;

use serenity::all::MessageBuilder;
use serenity::model::id::ChannelId;

macro_rules! log {
    ($($arg:tt)*) => {
        println!("RSS: {}", format_args!($($arg)*));
    };
}

// Originally this function was async but I had to switch to feed_rs to handle different feed types and it's badly designed
async fn fetch_feed<T: reqwest::IntoUrl>(url: T) -> Result<feed_rs::model::Feed, Box<dyn Error>> {
    let content = reqwest::get(url.as_str().to_string()).await?.bytes().await?.reader();
    let parser = feed_rs::parser::Builder::new().base_uri(Some(&url.as_str())).build();
    let feed = parser.parse(content)?;
    // log!("Feed {} has {} entries.", &url.as_str(), feed.entries.len());
    Ok(feed)
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
            log!("failed to fetch rss feed: {why:?}");
            return ret;
        },
        Ok(channel) => {
            for item in channel.entries {
                // Rust hasn't implemented this RFC from 6+ years ago
                // https://rust-lang.github.io/rfcs/2497-if-let-chains.html
                if !item.links.is_empty() {
                    if let Some(date) = item.published {
                        let ts = date.timestamp();
                        if ts <= 0 {
                            log!("failed to parse publish date: {date:?}");
                            return vec![];
                        }
                        
                        if ts > max_ts {
                            max_ts = ts;
                        }

                        if ts > feed.last_item_ts {
                            ret.push(RssFeedEntry {
                                link: item.links[0].href.clone(),
                                title: match item.title {
                                    Some(t) => t.content,
                                    None => "".to_string(),
                                },
                            });
                        }
                        
                    } else {
                        log!("Item missing pub_date");
                    }
                } else {
                    log!("Item missing link");
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
                    log!("failed to send message feed: {why:?}");
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
        // Lock mutex
        let mut feeds = self.feeds.lock().await;

        // Parse json file
        let contents = match std::fs::read_to_string(
            env::var("HOME").expect("no home directory")
            + "/.bot_rs/rss.json"
        ) {
            Err(what) => {
                log!("Can't open rss.json {}", what);
                return;
            },
            Ok(cont) => cont,
        };
        if contents.is_empty() {
            log!("rss.json is empty");
            return;
        }

        // Convert from json
        let rules_json = json::parse(&contents).expect("invalid rss rules");
        let mut new_feeds: Vec<RssFeedStatus> = vec![];
        for r in rules_json.members() {
            let url = r["url"].to_string();
            let last_item_ts = r["last_item_ts"].as_i64().expect("last_item_ts must be i64");
            let channels = r["channels"].members().map(
                |cid| ChannelId::new(cid.as_u64().expect("channel id must be u64"))
            ).collect::<Vec<ChannelId>>();
            new_feeds.push(RssFeedStatus{ 
                url: url.to_string(),
                last_item_ts: last_item_ts,
                channels: channels,
            });
        }

        // Update feeds state
        let _ = mem::replace(&mut *feeds, new_feeds);
        log!("loaded {} rules", feeds.len());
    }

    /// Write to db
    async fn store(&self) {
        // Covnvert to json
        let mut jv: json::JsonValue = array![];
        for f in self.feeds.lock().await.iter_mut() {
            jv.push(object! {
                url: f.url.clone(),
                last_item_ts: f.last_item_ts,
                channels: f.channels.iter().map(|c| c.get()).collect::<Vec<u64>>(),
            }).expect("impossible");
        }

        // Convert to json
        fs::write(
            env::var("HOME").expect("no home directory") + "/.bot_rs/rss.json",
            jv.dump(),
        ).expect("cannot write to rss.json");
    }

    /// Add a new rule
    pub async fn subscribe(&self, channel_id: ChannelId, feed_url: String) -> String {
        // self.load().await;

        let mut ret = false;
        for f in self.feeds.lock().await.iter_mut() {
            if f.url == feed_url {
                if f.channels.contains(&channel_id) {
                    return "Already subscribed".to_string();
                }
                f.channels.push(channel_id);
                ret = true;
                break;
            }
        }
        if ret {            
            self.store().await;
            return "Added subscription".to_string();
        }

        // Check feed is valid
        if let Err(why) = fetch_feed(&feed_url).await {
            log!("Failed to fetch user-requested feed: {feed_url:?} -- {why:?}");
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

    pub async fn unsubscribe(&self, channel_id: ChannelId, feed_url: String) -> String {
        // TODO There should be no need for these state variables and ugly logic
        let mut index = 0;
        let mut remove = false;
        let mut found = false;
        {
            let mut feeds = self.feeds.lock().await;
            for f in &mut *feeds {
                if f.url == feed_url {
                    f.channels = f.channels.clone().into_iter().filter(|f| *f != channel_id).collect();
                    remove = f.channels.is_empty();
                    found = true;
                    break;
                }
                index += 1;
            }

            if !found {
                return "not found".to_string();
            }
            if remove {
                feeds.remove(index);
            }
        }

        self.store().await;
        "unsubscribed".to_string()
    }

    pub async fn channel_subs(&self, channel_id: ChannelId) -> String {
        let mut ret: String = "".to_string();
        for f in self.feeds.lock().await.iter() {
            if f.channels.contains(&channel_id) {
                ret += "- ";
                ret += &f.url;
                ret += "\n";
            }
        }
        return ret;
    }

    async fn cron(&self, http: &Arc<serenity::http::Http>) {
        // self.load().await;
        // TODO do these in parallel
        for f in self.feeds.lock().await.iter_mut() {
            f.fetch(http).await;
        }

        // Check again every 5 mins
        tokio::time::sleep(Duration::from_secs(60*5)).await;
        Box::pin(self.cron(http)).await;
    }

    pub async fn start(rssmgr: &Arc<RssFeeds>, http: &Arc<serenity::http::Http>) {
        let httpc = Arc::clone(http);

        let r2 = Arc::clone(rssmgr);
        r2.load().await;

        let _t = tokio::spawn(async move {
            r2.cron(&httpc).await;
        });
    }

    pub fn new() -> RssFeeds {
        RssFeeds { feeds: Arc::new( Mutex::new(  vec![] ) ) }
    }


}

