use chrono::{Duration, Utc};
use dotenv::dotenv;
use matrix_sdk::{
    ruma::{
        api::client::message,
        events::room::message::{
            MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
        },
    },
    Room, RoomState,
};
use std::env;

mod cal;
use cal::{get_calendar_events, CalDavCredentials};
mod event;
use event::EventTime;
mod matrix;
mod parser;
use matrix::{login, restore_session, sync, MatrixCredentials};

// Basically the goal for the Matrix bot is
// 1. To respond with a list of events to the !calendar command
// 2. To post a list of upcoming events once a week
//
// Both of these require:
// 1. Requesting events from the calendar, at least for the upcoming period ✅
// 2. Filtering those events so only those on the day of the post and in
//     the next 7 days are included
// 3. Displaying the Date and Time in a nice human-readable way, in
//     the correct timezone ✅
// 4. Ordering them chronologically ✅
// 5. Displaying them neatly in a message

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

    sync(client, sync_token, &session_file, on_room_message)
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
                format!("{} – {}", format_datetime(start), format_datetime(end))
            }
        }
        (EventTime::DateTime(start_datetime), EventTime::DateTime(end_datetime)) => {
            if start_datetime.date_naive() == end_datetime.date_naive() {
                format!(
                    "{} – {}",
                    start_datetime.format("%-I:%M %p").to_string(),
                    format_datetime(end)
                )
            } else {
                format!("{} – {}", format_datetime(start), format_datetime(end))
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
    if room.state() != RoomState::Joined {
        return;
    }

    let MessageType::Text(text_content) = &event.content.msgtype else {
        return;
    };

    let caldav_credentials = CalDavCredentials::new(
        env::var("CALDAV_SERVER_URL")
            .expect("CALDAV_SERVER_URL must be set")
            .parse()
            .unwrap(),
        env::var("CALDAV_USERNAME").expect("CALDAV_USERNAME must be set"),
        env::var("CALDAV_PASSWORD").expect("CALDAV_PASSWORD must be set"),
    );
    // let start = "20240617T000000Z";
    // let end = "20240619T235959Z";

    let start = Utc::now();
    let window = Duration::days(7);
    let end = start + window;

    // get the calendar events from caldav calendar
    let events = get_calendar_events(caldav_credentials, &start, &end).await;

    let mut message = String::new();

    for event in events.unwrap() {
        message = message
            + &format!(
                "{}: \n{}\n",
                event.name(),
                format_event_times(event.dtstart(), event.dtend())
            );
    }

    if text_content.body.contains("!calendar") {
        let content = RoomMessageEventContent::text_plain(message);

        println!("sending");

        // Send our message to the room we found the "!calendar" command in
        room.send(content).await.unwrap();

        println!("message sent");
    }

    let room_name = match room.display_name().await {
        Ok(room_name) => room_name.to_string(),
        Err(error) => {
            println!("Error getting room display name: {error}");
            // Let's fallback to the room ID.
            room.room_id().to_string()
        }
    };

    println!("[{room_name}] {}: {}", event.sender, text_content.body)
}
