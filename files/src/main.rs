use std::env;

use dotenv::dotenv;

mod cal;
use cal::{initial_sync, CalDavCredentials};

mod matrix;
use matrix::{login, restore_session, sync, MatrixCredentials};

const CACHE_FOLDER: &str = "test_cache/provider_sync";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Load the environment variables from a .env file.
    dotenv().ok();
    let matrix_credentials = MatrixCredentials {
        homeserver: env::var("MATRIX_SERVER_URL").expect("MATRIX_SERVER_URL must be set"),
        username: env::var("MATRIX_BOT_USERNAME").expect("MATRIX_BOT_USERNAME must be set"),
        password: env::var("MATRIX_BOT_PASSWORD").expect("MATRIX_BOT_PASSWORD must be set"),
    };

    let caldav_credentials = CalDavCredentials {
        username: env::var("CALDAV_USERNAME").expect("CALDAV_USERNAME must be set"),
        password: env::var("CALDAV_PASSWORD").expect("CALDAV_PASSWORD must be set"),
        server_url: env::var("CALDAV_SERVER_URL").expect("CALDAV_SERVER_URL must be set"),
    };

    // The folder containing this example's data.
    let data_dir = dirs::data_dir()
        .expect("no data_dir directory found")
        .join("persist_session");
    // The file where the session is persisted.
    let session_file = data_dir.join("session");

    // initial sync with caldav server
    let mut provider = initial_sync(CACHE_FOLDER, caldav_credentials).await;

    // login_and_sync(homeserver_url, username, password).await?;

    let (client, sync_token) = if session_file.exists() {
        restore_session(&session_file).await?
    } else {
        (
            login(&data_dir, &session_file, matrix_credentials).await?,
            None,
        )
    };

    sync(client, sync_token, &session_file)
        .await
        .map_err(Into::into)
}
