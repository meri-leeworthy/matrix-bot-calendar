//! Calendar events (iCal `VEVENT` items)

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventTime {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

impl EventTime {
    pub fn as_date(&self) -> Option<&NaiveDate> {
        match self {
            EventTime::Date(date) => Some(date),
            _ => None,
        }
    }

    // pub fn as_datetime(&self) -> Option<&DateTime<Utc>> {
    //     match self {
    //         EventTime::DateTime(datetime) => Some(datetime),
    //         _ => None,
    //     }
    // }
}

impl Ord for EventTime {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (EventTime::Date(d1), EventTime::Date(d2)) => d1.cmp(d2),
            (EventTime::DateTime(dt1), EventTime::DateTime(dt2)) => dt1.cmp(dt2),
            (EventTime::Date(d), EventTime::DateTime(dt)) => match d.and_hms_opt(0, 0, 0) {
                Some(d) => d.cmp(&dt.naive_utc()),
                None => Ordering::Less,
            },
            (EventTime::DateTime(dt), EventTime::Date(d)) => {
                dt.naive_utc().cmp(match &d.and_hms_opt(0, 0, 0) {
                    Some(d) => d,
                    None => return Ordering::Greater,
                })
            }
        }
    }
}

impl PartialOrd for EventTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for EventTime {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for EventTime {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    uid: String,
    name: String,
    dtstart: EventTime,
    dtend: EventTime,
    location: Option<String>,
    description: Option<String>,
    last_modified: DateTime<Utc>,
    creation_date: Option<DateTime<Utc>>,
    url: Url,
}

impl Event {
    pub fn new_timed(
        name: String,
        uid: String,
        dtstart: DateTime<Utc>,
        dtend: DateTime<Utc>,
        location: Option<String>,
        description: Option<String>,
        url: Url,
        last_modified: DateTime<Utc>,
        creation_date: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            name,
            uid,
            dtstart: EventTime::DateTime(dtstart),
            dtend: EventTime::DateTime(dtend),
            location,
            description,
            last_modified,
            creation_date,
            url,
        }
    }

    pub fn new_all_day(
        name: String,
        uid: String,
        dtstart: NaiveDate,
        dtend: NaiveDate,
        location: Option<String>,
        description: Option<String>,
        url: Url,
        last_modified: DateTime<Utc>,
        creation_date: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            name,
            uid,
            dtstart: EventTime::Date(dtstart),
            dtend: EventTime::Date(dtend),
            location,
            description,
            last_modified,
            creation_date,
            url,
        }
    }

    // pub fn url(&self) -> &Url {
    //     &self.url
    // }

    // pub fn uid(&self) -> &str {
    //     &self.uid
    // }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dtstart(&self) -> &EventTime {
        &self.dtstart
    }

    pub fn dtend(&self) -> &EventTime {
        &self.dtend
    }

    // pub fn location(&self) -> Option<&String> {
    //     self.location.as_ref()
    // }

    // pub fn description(&self) -> Option<&String> {
    //     self.description.as_ref()
    // }

    // pub fn last_modified(&self) -> &DateTime<Utc> {
    //     &self.last_modified
    // }

    // pub fn creation_date(&self) -> Option<&DateTime<Utc>> {
    //     self.creation_date.as_ref()
    // }

    // #[cfg(any(test, feature = "integration_tests"))]
    // pub fn has_same_observable_content_as(&self, other: &Event) -> bool {
    //     self.uid == other.uid
    //         && self.name == other.name
    //         && self.dtstart == other.dtstart
    //         && self.dtend == other.dtend
    //         && self.location == other.location
    //         && self.description == other.description
    //         && self.last_modified == other.last_modified
    // }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        self.dtstart.cmp(&other.dtstart)
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

impl Eq for Event {}
