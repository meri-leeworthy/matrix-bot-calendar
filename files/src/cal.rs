use chrono::{DateTime, Utc};
use core::panic;
use minidom::Element;
use reqwest::header::CONTENT_TYPE;
use std::error::Error;
use url;

use crate::event::Event;
use crate::parser;

fn main() {
    panic!("This file is not supposed to be executed");
}

#[derive(Clone, Debug)]
pub struct CalDavCredentials {
    url: url::Url,
    username: String,
    password: String,
}

impl CalDavCredentials {
    pub fn new(url: url::Url, username: String, password: String) -> Self {
        Self {
            url,
            username,
            password,
        }
    }

    pub fn url(&self) -> &url::Url {
        &self.url
    }
    pub fn username(&self) -> &String {
        &self.username
    }
    pub fn password(&self) -> &String {
        &self.password
    }
}

/// Initializes a Provider, and run an initial sync from the server
pub async fn get_calendar_events(
    credentials: CalDavCredentials,
    start: &DateTime<Utc>,
    end: &DateTime<Utc>,
) -> Result<Vec<Event>, String> {
    let cal_body = format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<C:calendar-query xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop xmlns:D="DAV:">
    <D:getetag/>
    <C:calendar-data>
      <C:comp name="VCALENDAR">
        <C:comp name="VEVENT"/>
      </C:comp>
    </C:calendar-data>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{start}" end="{end}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>
"#,
        start = start.format("%Y%m%dT%H%M%SZ").to_string(),
        end = end.format("%Y%m%dT%H%M%SZ").to_string()
    );

    log::info!("Requesting items from my calendar");
    let responses_result =
        sub_request_and_extract_elems(&credentials, "REPORT", cal_body, "response").await;
    let responses = match responses_result {
        Ok(responses) => responses,
        Err(err) => {
            log::error!("Error: {}", err);
            return Ok(Vec::new());
        }
    };

    log::debug!("Response: {:?}", responses);

    let calendar_data_vec = extract_calendar_data(&responses);
    log::debug!("calendar_data_vec: {:?}", calendar_data_vec.len());

    let mut events = Vec::new();

    for calendar_data in calendar_data_vec {
        log::debug!("calendar_data: {}", calendar_data);
        let resource_url = credentials.url().clone();
        match parser::parse(&calendar_data, resource_url) {
            Ok(parsed) => events.push(parsed),
            Err(err) => {
                log::error!("Error: {}", err);
            }
        };
    }

    log::debug!("events: {:?}", events.len());

    events.sort();

    Ok(events)
}

// Function to extract the calendar data from the XML element
fn extract_calendar_data(root: &Vec<Element>) -> Vec<String> {
    let mut calendar_data_vec = Vec::new();

    for response in root {
        if response.name() == "response" && response.ns() == "DAV:" {
            for propstat in response.children() {
                if propstat.name() == "propstat" && propstat.ns() == "DAV:" {
                    for prop in propstat.children() {
                        if prop.name() == "prop" && prop.ns() == "DAV:" {
                            for calendar_data in prop.children() {
                                if calendar_data.name() == "calendar-data"
                                    && calendar_data.ns() == "urn:ietf:params:xml:ns:caldav"
                                {
                                    calendar_data_vec.push(calendar_data.text());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    calendar_data_vec
}

pub async fn sub_request(
    resource: &CalDavCredentials,
    method: &str,
    body: String,
    depth: u32,
) -> Result<String, Box<dyn Error>> {
    let method = method.parse().expect("invalid method name");

    let res = reqwest::Client::new()
        .request(method, resource.url().clone())
        .header("Depth", depth)
        .header(CONTENT_TYPE, "application/xml")
        .basic_auth(resource.username(), Some(resource.password()))
        .body(body)
        .send()
        .await?;

    let status = res.status();
    let text = res.text().await?;

    log::debug!("Response status: {:?}", status);
    log::debug!("Response body: {}", text);

    if status.is_success() == false {
        return Err(format!("Unexpected HTTP status code {:?}", status).into());
    }

    // log::debug!("Response: {}", text);

    Ok(text)
}

/// Walks an XML tree and returns every element that has the given name
pub fn find_elems<S: AsRef<str>>(root: &Element, searched_name: S) -> Vec<&Element> {
    let searched_name = searched_name.as_ref();
    let mut elems: Vec<&Element> = Vec::new();

    for el in root.children() {
        if el.name() == searched_name {
            elems.push(el);
        } else {
            let ret = find_elems(el, searched_name);
            elems.extend(ret);
        }
    }
    elems
}

pub async fn sub_request_and_extract_elems(
    resource: &CalDavCredentials,
    method: &str,
    body: String,
    item: &str,
) -> Result<Vec<Element>, Box<dyn Error>> {
    let text = sub_request(resource, method, body, 1).await?;

    let element: &Element = &text.parse()?;
    // log::debug!("sub request for {}", resource.url());
    // log::debug!("Response: {:?}", text);
    Ok(find_elems(&element, item)
        .iter()
        .map(|elem| (*elem).clone())
        .collect())
}
