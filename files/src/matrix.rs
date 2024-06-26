use matrix_sdk::{
    config::SyncSettings,
    event_handler::{EventHandler, SyncEvent},
    matrix_auth::MatrixSession,
    ruma::{api::client::filter::FilterDefinition, events::room::member::StrippedRoomMemberEvent},
    Client, Error, LoopCtrl, Room,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::fs;

use std::{path::Path, sync::Arc};

use std::path::PathBuf;

/// The data needed to re-build a client.
#[derive(Debug, Serialize, Deserialize)]
struct ClientSession {
    /// The URL of the homeserver of the user.
    homeserver: String,

    /// The path of the database.
    db_path: PathBuf,

    /// The passphrase of the database.
    passphrase: String,
}

/// The full session to persist.
#[derive(Debug, Serialize, Deserialize)]
struct FullSession {
    /// The data to re-build the client.
    client_session: ClientSession,

    /// The Matrix user session.
    user_session: MatrixSession,

    /// The latest sync token.
    ///
    /// It is only needed to persist it when using `Client::sync_once()` and we
    /// want to make our syncs faster by not receiving all the initial sync
    /// again.
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_token: Option<String>,
}

pub struct MatrixCredentials {
    pub username: String,
    pub password: String,
    pub homeserver: String,
}

/// Restore a previous session.
pub async fn restore_session(session_file: &Path) -> anyhow::Result<(Client, Option<String>)> {
    log::info!(
        "Previous session found in '{}'",
        session_file.to_string_lossy()
    );

    // The session was serialized as JSON in a file.
    let serialized_session = fs::read_to_string(session_file).await?;
    let FullSession {
        client_session,
        user_session,
        sync_token,
    } = serde_json::from_str(&serialized_session)?;

    // Build the client with the previous settings from the session.
    let client = Client::builder()
        .homeserver_url(client_session.homeserver)
        .sqlite_store(client_session.db_path, Some(&client_session.passphrase))
        .build()
        .await?;

    log::info!("Restoring session for {}…", user_session.meta.user_id);

    // Restore the Matrix user session.
    client.restore_session(user_session).await?;

    Ok((client, sync_token))
}

/// Login with a new device.
pub async fn login(
    data_dir: &Path,
    session_file: &Path,
    credentials: MatrixCredentials,
) -> anyhow::Result<Client> {
    log::info!("No previous session found, logging in…");

    let (client, client_session) = build_client(data_dir, credentials.homeserver).await?;
    let matrix_auth = client.matrix_auth();

    match matrix_auth
        .login_username(&credentials.username, &credentials.password)
        .initial_device_display_name("persist-session client")
        .await
    {
        Ok(_) => {
            log::info!("Logged in as {}", credentials.username);
        }
        Err(error) => {
            log::error!("Error logging in: {error}");
            log::error!("Please try again\n");
        }
    }

    // Persist the session to reuse it later.
    // This is not very secure, for simplicity. If the system provides a way of
    // storing secrets securely, it should be used instead.
    // Note that we could also build the user session from the login response.
    let user_session = matrix_auth
        .session()
        .expect("A logged-in client should have a session");
    let serialized_session = serde_json::to_string(&FullSession {
        client_session,
        user_session,
        sync_token: None,
    })?;
    fs::write(session_file, serialized_session).await?;

    log::info!("Session persisted in {}", session_file.to_string_lossy());

    // After logging in, you might want to verify this session with another one (see
    // the `emoji_verification` example), or bootstrap cross-signing if this is your
    // first session with encryption, or if you need to reset cross-signing because
    // you don't have access to your old sessions (see the
    // `cross_signing_bootstrap` example).

    Ok(client)
}

/// Build a new client.
async fn build_client(
    data_dir: &Path,
    homeserver: String,
) -> anyhow::Result<(Client, ClientSession)> {
    let mut rng = thread_rng();

    // Generating a subfolder for the database is not mandatory, but it is useful if
    // you allow several clients to run at the same time. Each one must have a
    // separate database, which is a different folder with the SQLite store.
    let db_subfolder: String = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(7)
        .map(char::from)
        .collect();
    let db_path = data_dir.join(db_subfolder);

    // Generate a random passphrase.
    let passphrase: String = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    match Client::builder()
        .homeserver_url(&homeserver)
        // We use the SQLite store, which is enabled by default. This is the crucial part to
        // persist the encryption setup.
        // Note that other store backends are available and you can even implement your own.
        .sqlite_store(&db_path, Some(&passphrase))
        .build()
        .await
    {
        Ok(client) => {
            return Ok((
                client,
                ClientSession {
                    homeserver,
                    db_path,
                    passphrase,
                },
            ))
        }
        Err(error) => match &error {
            matrix_sdk::ClientBuildError::AutoDiscovery(_)
            | matrix_sdk::ClientBuildError::Url(_)
            | matrix_sdk::ClientBuildError::Http(_) => {
                log::error!("Error checking the homeserver: {error}");
                log::error!("Please try again\n");
                Err(error.into())
            }
            _ => {
                // Forward other errors, it's unlikely we can retry with a different outcome.
                Err(error.into())
            }
        },
    }
}

/// Setup the client to listen to new messages.
pub async fn sync<Ev, Ctx, H>(
    client: Arc<Client>,
    initial_sync_token: Option<String>,
    session_file: &Path,
    on_room_message: H,
) -> anyhow::Result<()>
where
    Ev: SyncEvent + DeserializeOwned + Send + 'static,
    Ctx: Send + Sync + 'static,
    H: EventHandler<Ev, Ctx> + Send + Sync + 'static,
{
    log::info!("Launching a first sync to ignore past messages…");

    // Enable room members lazy-loading, it will speed up the initial sync a lot
    // with accounts in lots of rooms.
    // See <https://spec.matrix.org/v1.6/client-server-api/#lazy-loading-room-members>.
    let filter = FilterDefinition::with_lazy_loading();

    let mut sync_settings = SyncSettings::default().filter(filter.into());

    // We restore the sync where we left.
    // This is not necessary when not using `sync_once`. The other sync methods get
    // the sync token from the store.
    if let Some(sync_token) = initial_sync_token {
        sync_settings = sync_settings.token(sync_token);
    }

    // Let's ignore messages before the program was launched.
    // This is a loop in case the initial sync is longer than our timeout. The
    // server should cache the response and it will ultimately take less time to
    // receive.
    loop {
        match client.sync_once(sync_settings.clone()).await {
            Ok(response) => {
                // This is the last time we need to provide this token, the sync method after
                // will handle it on its own.
                sync_settings = sync_settings.token(response.next_batch.clone());
                persist_sync_token(session_file, response.next_batch).await?;
                break;
            }
            Err(error) => {
                log::error!("An error occurred during initial sync: {error}");
                log::error!("Trying again…");
            }
        }
    }

    log::info!("The client is ready! Listening to new messages…");

    // Now that we've synced, let's attach a handler for incoming room messages.
    client.add_event_handler(on_room_message);
    client.add_event_handler(on_room_invite);

    // This loops until we kill the program or an error happens.
    client
        .sync_with_result_callback(sync_settings, |sync_result| async move {
            let response = sync_result?;

            // We persist the token each time to be able to restore our session
            persist_sync_token(session_file, response.next_batch)
                .await
                .map_err(|err| Error::UnknownError(err.into()))?;

            Ok(LoopCtrl::Continue)
        })
        .await?;

    Ok(())
}

/// Persist the sync token for a future session.
/// Note that this is needed only when using `sync_once`. Other sync methods get
/// the sync token from the store.
async fn persist_sync_token(session_file: &Path, sync_token: String) -> anyhow::Result<()> {
    let serialized_session = fs::read_to_string(session_file).await?;
    let mut full_session: FullSession = serde_json::from_str(&serialized_session)?;

    full_session.sync_token = Some(sync_token);
    let serialized_session = serde_json::to_string(&full_session)?;
    fs::write(session_file, serialized_session).await?;

    Ok(())
}

async fn on_room_invite(event: StrippedRoomMemberEvent, room: Room) {
    if event.state_key == *room.own_user_id() {
        log::info!("Accepting invite for room: {}", room.room_id());
        match room.join().await {
            Ok(_) => {
                log::info!("Joined room: {}", room.room_id());
            }
            Err(err) => {
                log::error!("Error joining room: {err}");
            }
        }
    }
}
