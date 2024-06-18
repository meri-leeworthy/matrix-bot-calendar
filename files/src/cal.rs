use core::panic;
use dotenv::dotenv;
use kitchen_fridge::cache::Cache;
use kitchen_fridge::client::Client;
use kitchen_fridge::traits::CalDavSource;
use kitchen_fridge::CalDavProvider;
use std::env;
use std::path::Path;

fn main() {
    panic!("This file is not supposed to be executed");
}

/// Initializes a Provider, and run an initial sync from the server
pub async fn initial_sync(cache_folder: &str) -> CalDavProvider {
    dotenv().ok();

    let (server_url, username, password) = match (
        env::var("CALDAV_SERVER_URL"),
        env::var("CALDAV_USERNAME"),
        env::var("CALDAV_PASSWORD"),
    ) {
        (Ok(h), Ok(u), Ok(p)) => (h, u, p),
        _ => {
            eprintln!("Environment variables CALDAV_SERVER_URL, CALDAV_USERNAME, and CALDAV_PASSWORD must be set.");
            panic!("Missing environment variables");
        }
    };

    let cache_path = Path::new(cache_folder);

    let client = Client::new(server_url, username, password).unwrap();
    let cache = match Cache::from_folder(&cache_path) {
        Ok(cache) => cache,
        Err(err) => {
            println!("Invalid cache file: {}. Using a default cache", err);
            Cache::new(&cache_path)
        }
    };
    let mut provider = CalDavProvider::new(client, cache);

    let cals = provider.local().get_calendars().await.unwrap();
    println!("---- Local items, before sync -----");
    kitchen_fridge::utils::print_calendar_list(&cals).await;

    println!("Starting a sync...");
    println!(
        "Depending on your RUST_LOG value, you may see more or less details about the progress."
    );
    // Note that we could use sync_with_feedback() to have better and formatted feedback
    if provider.sync().await == false {
        println!("Sync did not complete, see the previous log lines for more info. You can safely start a new sync.");
    }
    provider.local().save_to_folder().unwrap();

    println!("---- Local items, after sync -----");
    let cals = provider.local().get_calendars().await.unwrap();
    kitchen_fridge::utils::print_calendar_list(&cals).await;

    provider
}
