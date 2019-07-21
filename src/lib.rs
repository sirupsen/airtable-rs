//! # airtable
//! 
//! Rust wrapper for the Airtable API.  The official API's documentation can be
//! found [here](https://airtable.com/api). This is also where you can find your API
//! tokens. This is inspired by [Airrecord for Ruby](https://github.com/sirupsen/airrecord).
//! 
//! The wrapper is not complete, but has the basics and is easy to extend.
//! 
//! [Rustdocs](https://docs.rs/airtable/)
//! 
//! ### Installation
//! 
//! Add `airtable = "*"` to your `Cargo.toml`.
//! 
//! ### Example
//! 
//! ```
//! extern crate dotenv;
//! extern crate serde;
//! 
//! use dotenv::dotenv;
//! use std::env;
//! use serde::{Serialize, Deserialize};
//!
//! // You don't need to use dotenv. I use it here because it makes it much easier to test without
//! // publishing my keys to the kingdom :-)
//! dotenv().ok();
//!
//! // Define the schema in Airtable. You don't need to type out the full row schema.
//! // You can use the serde annotation of `default` if it's optional and rename columns,
//! // as I've done here to map from upper-case. You must define a string id identifier.
//! //
//! // In this case, I'm mapping words that I have highlighted on my kindle with the # of results
//! // on Google so I can choose which ones to learn first.
//! #[derive(Serialize, Deserialize, Debug, Default)]
//! struct Word {
//!     #[serde(default, skip_serializing)]
//!     id: String,
//!     #[serde(rename = "Word")]
//!     word: String,
//!     #[serde(rename = "Google")]
//!     google: i64,
//!     #[serde(rename = "Next", default)]
//!     next: bool,
//! }
//!
//! // We need to define two methods on the structure so that ids can be assigned to it.
//! //
//! // TODO: Convert this to be a `derive(Airtable)` and be automatically defined but panic if the
//! // `id` is not a member of the struct and is a String. Contributions welcome for this or
//! // another ergonomic solution.
//! impl airtable::Record for Word {
//!     fn set_id(&mut self, id: String) {
//!         self.id = id;
//!     }
//! 
//!     fn id(&self) -> &str {
//!         &self.id
//!     }
//! }
//!
//! // Define the base object to operate on.
//! let base = airtable::new::<Word>(
//!     &env::var("AIRTABLE_KEY").unwrap(),
//!     &env::var("AIRTABLE_BASE_WORDS_KEY").unwrap(),
//!     "Words",
//! );
//!
//! // Query on the base. This implements the Iterator Trait and will paginate when reaching a page
//! // boundary. If you remove the `take(200)`, it'll just paginate through everything.
//! let mut results: Vec<_> = base
//!     .query()
//!     .view("To Learn")
//!     .sort("Next", airtable::SortDirection::Descending)
//!     .sort("Google", airtable::SortDirection::Descending)
//!     .sort("Created", airtable::SortDirection::Descending)
//!     .formula("FIND(\"Harry Potter\", Source)")
//!     .into_iter()
//!     .take(200)
//!     .collect();
//!
//! // Pop the first element by taking ownership of it and print it
//! let mut word = results.remove(0);
//! println!("{:?}", word);
//!
//! // Toggle the flag and update the record.
//! word.next = !word.next;
//! base.update(&word);
//!
//! // Create a new word!
//! let mut new_word = Word {
//!     word: "lurid".to_string(),
//!     google: 6870000,
//!     next: true,
//!     // Set id to nil and other attributes we may not care about or not know yet.
//!     .. Default::default()
//! };
//!
//! println!("{:?}", base.create(&new_word));
//! ```
//! 
//! License: MIT
#![allow(dead_code)]
extern crate failure;
extern crate reqwest;
extern crate serde;
extern crate serde_json;

#[cfg(test)]
extern crate mockito;

use serde::{Serialize, Deserialize};
use failure::Error;
use reqwest::header;
use reqwest::Url;
use std::marker::PhantomData;

const URL: &str = "https://api.airtable.com/v0";

#[derive(Debug)]
pub struct Base<T: Record> {
    http_client: reqwest::Client,

    table: String,
    api_key: String,
    app_key: String,

    phantom: PhantomData<T>,
}

pub fn new<T>(api_key: &str, app_key: &str, table: &str) -> Base<T>
where
    T: Record,
{
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", &api_key)).expect("invalid api key"),
    );

    headers.insert(
        reqwest::header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").expect("invalid content type"),
    );

    let http_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("unable to create client");

    Base {
        http_client,
        api_key: api_key.to_owned(),
        app_key: app_key.to_owned(),
        table: table.to_owned(),
        phantom: PhantomData,
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct SRecord<T> {
    #[serde(default, skip_serializing)]
    id: String,
    fields: T,
}

#[derive(Deserialize, Debug)]
struct RecordPage<T> {
    records: Vec<SRecord<T>>,

    #[serde(default)]
    offset: String,
}

pub struct Paginator<'base, T: Record> {
    base: &'base Base<T>,
    // TODO: Move the offset to query_builder
    offset: Option<String>,
    iterator: std::vec::IntoIter<T>,
    query_builder: QueryBuilder<'base, T>,
}

impl<'base, T> Iterator for Paginator<'base, T>
where
    for<'de> T: Deserialize<'de>,
    T: Record,
{
    type Item = T;
    // This somewhat masks errors..
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iterator.next();
        if next.is_some() {
            return next;
        }

        if self.offset.is_none() {
            return None;
        }

        let mut url = Url::parse(&format!(
            "{}/{}/{}",
            URL, self.base.app_key, self.base.table
        ))
        .unwrap();
        url.query_pairs_mut()
            .append_pair("offset", self.offset.as_ref().unwrap());

        if self.query_builder.view.is_some() {
            url.query_pairs_mut()
                .append_pair("view", self.query_builder.view.as_ref().unwrap());
        }

        if self.query_builder.formula.is_some() {
            url.query_pairs_mut().append_pair(
                "filterByFormula",
                self.query_builder.formula.as_ref().unwrap(),
            );
        }

        if self.query_builder.sort.is_some() {
            for (i, ref sort) in self.query_builder.sort.as_ref().unwrap().iter().enumerate() {
                url.query_pairs_mut()
                    .append_pair(&format!("sort[{}][field]", i), &sort.0);
                url.query_pairs_mut()
                    .append_pair(&format!("sort[{}][direction]", i), &sort.1.to_string());
            }
        }

        // println!("{}", url);

        let mut response = self
            .base
            .http_client
            .get(url.as_str())
            .send()
            .ok()?;

        let results: RecordPage<T> = response.json().ok()?;

        if results.offset.is_empty() {
            self.offset = None;
        } else {
            self.offset = Some(results.offset);
        }

        let window: Vec<T> = results
            .records
            .into_iter()
            .map(|record| {
                let mut record_t: T = record.fields;
                record_t.set_id(record.id);
                record_t
            })
            .collect();

        self.iterator = window.into_iter();
        self.iterator.next()
    }
}

pub trait Record {
    fn set_id(&mut self, String);
    fn id(&self) -> &str;
}

pub enum SortDirection {
    Descending,
    Ascending,
}

impl ToString for SortDirection {
    fn to_string(&self) -> String {
        match self {
            SortDirection::Descending => String::from("desc"),
            SortDirection::Ascending => String::from("asc"),
        }
    }
}

pub struct QueryBuilder<'base, T: Record> {
    base: &'base Base<T>,

    fields: Option<Vec<String>>,
    view: Option<String>,
    formula: Option<String>,

    // TODO: Second value here should be an enum.
    sort: Option<Vec<(String, SortDirection)>>,
}

impl<'base, T> QueryBuilder<'base, T>
where
    for<'de> T: Deserialize<'de>,
    T: Record,
{
    pub fn view(mut self, view: &str) -> Self {
        self.view = Some(view.to_owned());
        self
    }

    pub fn formula(mut self, formula: &str) -> Self {
        self.formula = Some(formula.to_owned());
        self
    }

    pub fn sort(mut self, field: &str, direction: SortDirection) -> Self {
        match self.sort {
            None => {
                self.sort = Some(vec![(field.to_owned(), direction)]);
            }
            Some(ref mut sort) => {
                let tuple = (field.to_owned(), direction);
                sort.push(tuple);
            }
        };
        self
    }
}

impl<'base, T> IntoIterator for QueryBuilder<'base, T>
where
    for<'de> T: Deserialize<'de>,
    T: Record,
{
    type Item = T;
    type IntoIter = Paginator<'base, T>;

    fn into_iter(self) -> Self::IntoIter {
        Paginator {
            base: &self.base,
            offset: Some("".to_owned()),
            iterator: vec![].into_iter(),
            query_builder: self,
        }
    }
}

impl<T> Base<T>
where
    for<'de> T: Deserialize<'de>,
    T: Record,
{
    pub fn query(&self) -> QueryBuilder<T> {
        QueryBuilder {
            base: self,
            fields: None,
            view: None,
            formula: None,
            sort: None,
        }
    }

    pub fn create(&self, record: &T) -> Result<(), Error>
    where
        T: serde::Serialize,
    {
        let url = format!("{}/{}/{}", URL, self.app_key, self.table);

        let serializing_record = SRecord {
            id: String::new(),
            fields: record,
        };

        let json = serde_json::to_string(&serializing_record)?;

        self.http_client
            .post(&url)
            .body(json)
            .send()?
            .error_for_status()?;

        Ok(())
    }

    // TODO: Perhaps pass a mutable reference to allow updating computed fields when someone does
    // an update?
    //
    // TODO: Include the error body in the error.
    pub fn update(&self, record: &T) -> Result<(), Error>
    where
        T: serde::Serialize,
    {
        let url = format!("{}/{}/{}/{}", URL, self.app_key, self.table, record.id());

        let serializing_record = SRecord {
            id: record.id().to_owned(),
            fields: record,
        };

        let json = serde_json::to_string(&serializing_record)?;

        self.http_client
            .patch(&url)
            .body(json)
            .send()?
            .error_for_status()?;

        Ok(())
    }
}
