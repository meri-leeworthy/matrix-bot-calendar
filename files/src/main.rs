use chrono::{Datelike, Duration, Utc, Weekday};
use dotenv::dotenv;
use matrix_sdk::{
    ruma::{
        events::room::message::{
            MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
        },
        RoomId,
    },
    Client, Room, RoomState,
};
use std::{env, sync::Arc};

mod cal;
use cal::{get_calendar_events, CalDavCredentials};
mod event;
use event::EventTime;
mod matrix;
mod parser;
use matrix::{login, restore_session, sync, MatrixCredentials};
use std::time::Duration as StdDuration;
use tokio::time::{interval_at, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Load the environment variables from a .env file
    dotenv().ok();
    let matrix_credentials = MatrixCredentials {
        homeserver: env::var("MATRIX_SERVER_URL").expect("MATRIX_SERVER_URL must be set"),
        username: env::var("MATRIX_BOT_USERNAME").expect("MATRIX_BOT_USERNAME must be set"),
        password: env::var("MATRIX_BOT_PASSWORD").expect("MATRIX_BOT_PASSWORD must be set"),
    };

    // dry run to make sure env variables are set correctly
    get_events_message().await;

    // The folder containing persisted Matrix data
    let data_dir = dirs::data_dir()
        .expect("no data_dir directory found")
        .join("persist_session");
    // The file where the session is persisted
    let session_file = data_dir.join("session");

    let (client, sync_token) = if session_file.exists() {
        restore_session(&session_file).await?
    } else {
        (
            login(&data_dir, &session_file, matrix_credentials).await?,
            None,
        )
    };

    let client = &Arc::new(client);

    let room_ids = get_room_ids();
    for id in room_ids {
        let client_clone = Arc::clone(&client);
        RoomId::parse(&id).expect("MATRIX_ROOM_IDS must be a valid room ID");
        tokio::spawn(post_weekly_message(client_clone, id.clone()));
    }

    sync(client.clone(), sync_token, &session_file, on_room_message)
        .await
        .map_err(Into::into)
}

fn format_datetime(datetime: &EventTime) -> String {
    match datetime {
        EventTime::Date(date) => date.format("%A, %-d %B, %C%y").to_string(),
        EventTime::DateTime(datetime) => datetime.format("%-I:%M %p %A, %-d %B, %C%y").to_string(),
    }
}

fn format_event_times(start: &EventTime, end: &EventTime) -> String {
    match (start, end) {
        (EventTime::Date(start_date), EventTime::Date(end_date)) => {
            if *start_date == *end_date - Duration::days(1) {
                format!("{} – All Day", format_datetime(start))
            } else {
                format!("{} – {}\n", format_datetime(start), format_datetime(end))
            }
        }
        (EventTime::DateTime(start_datetime), EventTime::DateTime(end_datetime)) => {
            if start_datetime.date_naive() == end_datetime.date_naive() {
                format!(
                    "{} – {}\n",
                    start_datetime.format("%-I:%M %p").to_string(),
                    format_datetime(end)
                )
            } else {
                format!("{} – {}\n", format_datetime(start), format_datetime(end))
            }
        }
        (EventTime::Date(_), EventTime::DateTime(_))
        | (EventTime::DateTime(_), EventTime::Date(_)) => {
            "Invalid Date: Check Calendar".to_string()
        }
    }
}

/// Handle room messages.
async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
    // We only want to log text messages in joined rooms.
    if room.state() != RoomState::Joined
        || !get_room_ids()
            .iter()
            .any(|id| id == room.room_id().as_str())
    {
        return;
    }

    let MessageType::Text(text_content) = &event.content.msgtype else {
        return;
    };

    let (body, html_body) = get_events_message().await;

    if text_content.body.contains("!calendar") || text_content.body.contains("!cal") {
        let content = RoomMessageEventContent::text_html(body, html_body);

        log::info!("sending");

        // Send our message to the room we found the "!calendar" command in
        match room.send(content).await {
            Ok(_) => log::info!("message sent"),
            Err(error) => {
                log::error!("Error sending message: {error}");
            }
        }
    }

    let room_name = match room.display_name().await {
        Ok(room_name) => room_name.to_string(),
        Err(error) => {
            log::error!("Error getting room display name: {error}");
            // Let's fallback to the room ID.
            room.room_id().to_string()
        }
    };

    log::info!("[{room_name}] {}: {}", event.sender, text_content.body)
}

pub async fn post_weekly_message(client: Arc<Client>, room_id: String) {
    // Calculate the next instance of the specific time
    let now = Utc::now();
    let days_until_sunday =
        (Weekday::Sun.num_days_from_monday() + 7 - now.weekday().num_days_from_monday()) % 7;
    let next_sunday = now.date() + Duration::days(days_until_sunday.into());
    let target_time = match next_sunday.and_hms_opt(9, 0, 0) {
        Some(time) => time,
        None => {
            log::error!("Failed to calculate the next Sunday at 9:00 AM");
            return;
        }
    };

    let duration_until_target = (target_time - now)
        .to_std()
        .unwrap_or(StdDuration::from_secs(0));
    let start = Instant::now() + duration_until_target;

    let mut interval = interval_at(start, StdDuration::from_secs(7 * 24 * 60 * 60)); // 1 week interval

    loop {
        interval.tick().await;

        let room_id = RoomId::parse(&room_id).expect("Invalid room ID");

        // Post message to the room
        if let Some(room) = client.get_room(&room_id) {
            let (body, html_body) = get_events_message().await;
            let content = RoomMessageEventContent::text_html(body, html_body);

            match room.send(content).await {
                Ok(_) => log::info!("Weekly message sent"),
                Err(error) => {
                    log::error!("Error sending weekly message: {error}");
                }
            }
        } else {
            log::error!("Failed to find room with ID {}", room_id);
        }
    }
}

async fn get_events_message() -> (String, String) {
    let caldav_credentials = CalDavCredentials::new(
        env::var("CALDAV_SERVER_URL")
            .expect("CALDAV_SERVER_URL must be set")
            .parse()
            .expect("CALDAV_SERVER_URL must be a valid URL"),
        env::var("CALDAV_USERNAME").expect("CALDAV_USERNAME must be set"),
        env::var("CALDAV_PASSWORD").expect("CALDAV_PASSWORD must be set"),
    );
    // let start = "20240617T000000Z";
    // let end = "20240619T235959Z";

    let start = Utc::now();
    let window = Duration::days(7);
    let end = start + window;

    // get the calendar events from caldav calendar
    if let Ok(events) = get_calendar_events(caldav_credentials, &start, &end).await {
        let mut body = String::from("Upcoming Events");
        let mut html_body = String::from("<h3>Upcoming Events</h3><br />");

        if events.len() == 0 {
            body += "No events in the calendar this week";
            html_body += "<p>No events in the calendar this week</p>";
        };

        for event in events {
            body += &format!(
                "{}: \n{}\n\n",
                event.name(),
                format_event_times(event.dtstart(), event.dtend())
            );

            html_body += &format!(
                "<p><strong>{}</strong><br />{}</p>",
                event.name(),
                format_event_times(event.dtstart(), event.dtend())
            );
        }

        (body, html_body)
    } else {
        (
            "Failed to get calendar events".to_string(),
            "<p>Failed to get calendar events</p>".to_string(),
        )
    }
}

fn get_room_ids() -> Vec<String> {
    let room_ids = env::var("MATRIX_ROOM_IDS").expect("MATRIX_ROOM_IDS must be set");
    room_ids.split(',').map(|s| s.to_string()).collect()
}
