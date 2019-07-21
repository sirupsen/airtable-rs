# airtable

Rust wrapper for the Airtable API.  The official API's documentation can be
found [here](https://airtable.com/api). This is also where you can find your API
tokens. This is inspired by [Airrecord for Ruby](https://github.com/sirupsen/airrecord).

The wrapper is not complete, but has the basics and is easy to extend.

[Rustdocs](https://docs.rs/airtable/)

### Installation

Add `airtable = "*"` to your `Cargo.toml`.

### Example

```rust
extern crate dotenv;
extern crate serde;

use dotenv::dotenv;
use std::env;
use serde::{Serialize, Deserialize};

// You don't need to use dotenv. I use it here because it makes it much easier to test without
// publishing my keys to the kingdom :-)
dotenv().ok();

// Define the schema in Airtable. You don't need to type out the full row schema.
// You can use the serde annotation of `default` if it's optional and rename columns,
// as I've done here to map from upper-case. You must define a string id identifier.
//
// In this case, I'm mapping words that I have highlighted on my kindle with the # of results
// on Google so I can choose which ones to learn first.
#[derive(Serialize, Deserialize, Debug, Default)]
struct Word {
    #[serde(default, skip_serializing)]
    id: String,
    #[serde(rename = "Word")]
    word: String,
    #[serde(rename = "Google")]
    google: i64,
    #[serde(rename = "Next", default)]
    next: bool,
}

// We need to define two methods on the structure so that ids can be assigned to it.
//
// TODO: Convert this to be a `derive(Airtable)` and be automatically defined but panic if the
// `id` is not a member of the struct and is a String. Contributions welcome for this or
// another ergonomic solution.
impl airtable::Record for Word {
    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    fn id(&self) -> &str {
        &self.id
    }
}

// Define the base object to operate on.
let base = airtable::new::<Word>(
    &env::var("AIRTABLE_KEY").unwrap(),
    &env::var("AIRTABLE_BASE_WORDS_KEY").unwrap(),
    "Words",
);

// Query on the base. This implements the Iterator Trait and will paginate when reaching a page
// boundary. If you remove the `take(200)`, it'll just paginate through everything.
let mut results: Vec<_> = base
    .query()
    .view("To Learn")
    .sort("Next", airtable::SortDirection::Descending)
    .sort("Google", airtable::SortDirection::Descending)
    .sort("Created", airtable::SortDirection::Descending)
    .formula("FIND(\"Harry Potter\", Source)")
    .into_iter()
    .take(200)
    .collect();

// Pop the first element by taking ownership of it and print it
let mut word = results.remove(0);
println!("{:?}", word);

// Toggle the flag and update the record.
word.next = !word.next;
base.update(&word);

// Create a new word!
let mut new_word = Word {
    word: "lurid".to_string(),
    google: 6870000,
    next: true,
    // Set id to nil and other attributes we may not care about or not know yet.
    .. Default::default()
};

println!("{:?}", base.create(&new_word));
```

License: MIT
