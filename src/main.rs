use memmap2::MmapMut;
use std::{
    fs::File,
    mem,
    time::{Duration, SystemTime},
};
use twitch_eventsub::*;

#[repr(C)]
struct LatestStreamInfo {
    msgs_per_15s: u64,
    msgs_per_30s: u64,
    msgs_per_60s: u64,
    raid: u64,   // set to true for first 3 minutes of raid or something?
    follow: u64, // see above^ but for 15 seconds?
}

fn main() {
    let f = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("/tmp/strim-mmap-test.bin")
        .unwrap();

    let _ = f.set_len(mem::size_of::<LatestStreamInfo>() as u64);

    let latest_info = unsafe {
        let m = memmap2::MmapMut::map_mut(&f).unwrap();
        let m = Box::new(m);
        // DO NOTE I AM LEAKING MEMORY HERE ON PURPOSE
        // MAXI PLEASE DON'T FORGET IF THIS BECOMES
        // IMPORTANT LATER!!!
        // IT WILL BE SOOOO HARD TO DEBUG
        let m: &'static MmapMut = Box::leak(m);

        let latest_info: &mut LatestStreamInfo = std::mem::transmute(m.as_ptr());

        latest_info
    };

    let keys = TwitchKeys::from_secrets_env().unwrap();

    let twitch = TwitchEventSubApi::builder(keys)
        // sockets are used to read data from the request so a port
        // must be specified
        .set_redirect_url("https://localhost:3000")
        .generate_new_token_if_insufficent_scope(true)
        .generate_new_token_if_none(true)
        .generate_access_token_on_expire(true)
        .auto_save_load_created_tokens(".user_token.env", ".refresh_token.env")
        .add_subscription(Subscription::ChannelFollow)
        .add_subscriptions(vec![
            Subscription::ChannelRaid,
            Subscription::ChannelNewSubscription,
            Subscription::ChannelGiftSubscription,
            Subscription::ChannelResubscription,
            Subscription::ChannelCheer,
            Subscription::ChannelPointsCustomRewardRedeem,
            Subscription::ChannelPointsAutoRewardRedeem,
            Subscription::ChatMessage,
            Subscription::DeleteMessage,
            Subscription::AdBreakBegin,
        ]);

    let now = SystemTime::now();

    // Check for results or just unwrap if you are spicy!
    let mut api = {
        match twitch.build() {
            Ok(api) => api,
            Err(EventSubError::TokenMissingScope) => {
                panic!("Reauthorisation of token is required for the token to have all the requested subscriptions.");
            }
            Err(EventSubError::NoSubscriptionsRequested) => {
                panic!("No subscriptions passed into builder!");
            }
            Err(e) => {
                // some other error
                panic!("{:?}", e);
            }
        }
    };

    let mut messages_last_15s: Vec<Duration> = Vec::new();
    let mut messages_last_30s: Vec<Duration> = Vec::new();
    let mut messages_last_60s: Vec<Duration> = Vec::new();

    // users program main loop simulation
    loop {
        messages_last_15s
            .retain(|t| t.as_secs() > now.elapsed().unwrap().as_secs().saturating_sub(15));
        messages_last_30s
            .retain(|t| t.as_secs() > now.elapsed().unwrap().as_secs().saturating_sub(30));
        messages_last_60s
            .retain(|t| t.as_secs() > now.elapsed().unwrap().as_secs().saturating_sub(60));

        latest_info.msgs_per_15s = messages_last_15s.len() as u64;
        latest_info.msgs_per_30s = messages_last_30s.len() as u64;
        latest_info.msgs_per_60s = messages_last_60s.len() as u64;

        if latest_info.follow < now.elapsed().unwrap().as_secs().saturating_sub(60) {
            latest_info.follow = 0;
        }

        if latest_info.raid < now.elapsed().unwrap().as_secs().saturating_sub(60 * 5) {
            latest_info.raid = 0;
        }

        // Set duration to ZERO for non blocking for loop of messages
        // Recommended for most setups
        // If you are not running this inside a game and just byitself
        // Such as a chat bot, setting this to 1 millis seems to be good
        let responses = api.receive_messages(Duration::from_millis(100));
        for response in responses {
            match response {
                ResponseType::Event(event) => {
                    match event {
                        Event::ChatMessage(_) => {
                            messages_last_15s.push(now.elapsed().unwrap());
                            messages_last_30s.push(now.elapsed().unwrap());
                            messages_last_60s.push(now.elapsed().unwrap());
                        }
                        Event::Follow(_) => {
                            latest_info.follow = now.elapsed().unwrap().as_secs();
                        }
                        Event::Raid(_) => {
                            latest_info.raid = now.elapsed().unwrap().as_secs();
                        }
                        Event::PointsCustomRewardRedeem(redeem) => {
                            println!("{:#?}", redeem);
                        }
                        Event::ChannelPointsAutoRewardRedeem(redeem) => {
                            println!("{:#?}", redeem);
                        }
                        _ => {
                            // Events that you don't care about or are not subscribed to, can be ignored.
                        }
                    }
                }
                ResponseType::Close => println!("Twitch requested socket close."),
                _ => {}
            }
        }
    }
}
