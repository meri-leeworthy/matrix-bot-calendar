//! A module to parse ICal files

use crate::event::{Event, EventTime};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use ical::parser::ical::component::{IcalCalendar, IcalEvent};
use std::error::Error;
use url::Url;

/// Parse an iCal file into the internal representation [`crate::Event`]
pub fn parse(content: &str, item_url: Url) -> Result<Event, Box<dyn Error>> {
    let mut reader = ical::IcalParser::new(content.as_bytes());
    let parsed_item = match reader.next() {
        None => return Err(format!("Invalid iCal data to parse for item {}", item_url).into()),
        Some(item) => match item {
            Err(err) => {
                return Err(
                    format!("Unable to parse iCal data for item {}: {}", item_url, err).into(),
                )
            }
            Ok(item) => item,
        },
    };

    let event = assert_single_type(parsed_item)?;

    let mut name = None;
    let mut uid = None;
    let mut dtstart = None;
    let mut dtend = None;
    let mut location = None;
    let mut description = None;
    let mut last_modified = None;
    let mut creation_date = None;
    let mut extra_parameters = Vec::new();

    for prop in &event.properties {
        match prop.name.as_str() {
            "SUMMARY" => name = prop.value.clone(),
            "UID" => uid = prop.value.clone(),
            "DTSTART" => dtstart = parse_event_time_from_property(&prop.value),
            "DTEND" => dtend = parse_event_time_from_property(&prop.value),
            "LOCATION" => location = prop.value.clone(),
            "DESCRIPTION" => description = prop.value.clone(),
            "LAST-MODIFIED" => last_modified = parse_date_time_from_property(&prop.value),
            "CREATED" => creation_date = parse_date_time_from_property(&prop.value),
            _ => {
                // This field is not supported. Let's store it anyway, so that we are able to re-create an identical iCal file
                extra_parameters.push(prop.clone());
            }
        }
    }

    let name = name.ok_or_else(|| format!("Missing name for item {}", item_url))?;
    let uid = uid.ok_or_else(|| format!("Missing UID for item {}", item_url))?;
    let dtstart = dtstart.ok_or_else(|| format!("Missing DTSTART for item {}", item_url))?;
    let dtend = dtend.ok_or_else(|| format!("Missing DTEND for item {}", item_url))?;
    let last_modified = last_modified.ok_or_else(|| {
        format!(
            "Missing LAST-MODIFIED for item {}, but this is required by RFC5545",
            item_url
        )
    })?;

    let event = match dtstart {
        EventTime::DateTime(dtstart) => match dtend {
            EventTime::DateTime(dtend) => Event::new_timed(
                name,
                uid,
                dtstart,
                dtend,
                location,
                description,
                item_url,
                last_modified,
                creation_date,
            ),
            EventTime::Date(_) => {
                return Err(format!(
                    "DTEND for item {} is a date, but DTSTART is a datetime",
                    item_url
                )
                .into())
            }
        },
        EventTime::Date(_) => match dtend {
            EventTime::DateTime(_) => {
                return Err(format!(
                    "DTSTART for item {} is a date, but DTEND is a datetime",
                    item_url
                )
                .into())
            }
            EventTime::Date(dtend) => match dtstart.as_date() {
                Some(dtstart) => {
                    if dtstart > &dtend {
                        return Err(format!("DTSTART for item {} is after DTEND", item_url).into());
                    }
                    let dtstart = dtstart.to_owned();

                    Event::new_all_day(
                        name,
                        uid,
                        dtstart,
                        dtend,
                        location,
                        description,
                        item_url,
                        last_modified,
                        creation_date,
                    )
                }
                None => {
                    return Err(format!(
                        "DTSTART for item {} is a datetime, but DTEND is a date",
                        item_url
                    )
                    .into());
                }
            },
        },
    };

    // What to do with multiple items?
    if reader.next().map(|r| r.is_ok()) == Some(true) {
        return Err("Parsing multiple items are not supported".into());
    }

    Ok(event)
}

// Function to parse both datetime and date formats

fn parse_date_time(dt: &str) -> Result<DateTime<Utc>, chrono::format::ParseError> {
    match Utc.datetime_from_str(dt, "%Y%m%dT%H%M%SZ") {
        Ok(datetime) => Ok(datetime),
        Err(err) => Err(err),
    }
}

fn parse_date_time_from_property(value: &Option<String>) -> Option<DateTime<Utc>> {
    value.as_ref().and_then(|s| {
        parse_date_time(s)
            .map_err(|err| {
                log::warn!("Invalid timestamp: {}", s);
                err
            })
            .ok()
    })
}

fn parse_event_time(dt: &str) -> Result<EventTime, chrono::format::ParseError> {
    match Utc.datetime_from_str(dt, "%Y%m%dT%H%M%SZ") {
        Ok(datetime) => Ok(EventTime::DateTime(datetime)),
        Err(_) => match Utc.datetime_from_str(dt, "%Y%m%dT%H%M%S") {
            Ok(datetime) => Ok(EventTime::DateTime(datetime)),
            Err(_) => match NaiveDate::parse_from_str(dt, "%Y%m%d") {
                Ok(date) => Ok(EventTime::Date(date)),
                Err(err) => Err(err),
            },
        },
    }
}

fn parse_event_time_from_property(value: &Option<String>) -> Option<EventTime> {
    value.as_ref().and_then(|s| {
        parse_event_time(s)
            .map_err(|err| {
                log::warn!("Invalid timestamp: {}", s);
                err
            })
            .ok()
    })
}

fn assert_single_type(item: IcalCalendar) -> Result<IcalEvent, Box<dyn Error>> {
    let n_events = item.events.len();
    let n_todos = item.todos.len();
    let n_journals = item.journals.len();

    if n_events == 1 {
        if n_todos != 0 || n_journals != 0 {
            return Err("Only a single TODO or a single EVENT is supported".into());
        } else {
            return Ok(item.events[0].clone());
        }
    }

    return Err("Only a single EVENT is supported".into());
}
