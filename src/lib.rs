//! Simple Yandex ClickHouse client
//!
//!
//! How to use:
//! ```
//! use klickenhaus::ClickhouseClient;
//!
//! let ch = ClickhouseClient::new("http://localhost:8123");
//! assert!(ch.ping());
//! ch.query("DROP TABLE IF EXISTS test_table").expect("query failed");
//! ch.query("CREATE TABLE IF NOT EXISTS test_table (a Int32, b Enum8('a' = 1, 'b' = 2), c String, d Array(String)) ENGINE=Log").expect("query failed");
//! ch.query("INSERT INTO test_table (a, b, c, d) VALUES (10, 'a', 'test', ['abc', 'def']), (20, 'b', 'test2\ttest3\ntest4\rtest5', ['a\r\rbc', 'def', 'efg'])").expect("query failed");
//! ch.insert("INSERT INTO test_table (a, b, c, d)")
//! // Insert by columns
//!     .column(vec![50, 60, 70, 80])
//!     .column(vec!["a", "b", "a", "b"])
//!     .column(vec!["str1", "str2", "str3", "str4"])
//!     .column(vec![vec!["aaa", "bbb", "ccc"], vec!["ee", "ffff"], vec![], vec!["yo"]])
//! // Or rows
//!     .row((90, "a", "string", vec!["1", "2", "3"])).expect("can't insert row")
//!     .exec().expect("query failed");
//! assert_eq!(7, ch.select_row("SELECT count(*) FROM test_table").expect("query")[0].parse::<u32>().expect("count value"));
//! for row in ch.select("SELECT * FROM test_table").expect("query failed") {
//!     println!("'{}', '{}', '{}', {}", &row[0], &row[1], &row[2], &row[3]);
//! }
//! ch.query("DROP TABLE test_table").expect("query failed");
//! ```
//!

mod generated;
pub use crate::generated::*;

use failure::{bail, format_err, Error};
use std::borrow::Cow;
use std::{fmt::Write, ops::Index};

type R<T> = Result<T, Error>;

pub struct ClickhouseClient {
    url: String,
}

impl ClickhouseClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
        }
    }

    pub fn ping(&self) -> bool {
        reqwest::get(&self.url)
            .and_then(|mut r| r.text())
            .map(|t| t == "Ok.\n")
            .unwrap_or(false)
    }

    pub fn query(&self, query: &str) -> R<()> {
        let mut resp = reqwest::Client::new()
            .post(&self.url)
            .body(query.as_bytes().to_owned())
            .send()?;
        if resp.status().as_u16() != 200 {
            bail!("Query failed: {}", resp.text()?);
        }
        Ok(())
    }

    pub fn select(&self, query: &str) -> R<ClickhouseRows> {
        let mut resp = reqwest::Client::new()
            .post(&self.url)
            .body(query.as_bytes().to_owned())
            .send()?;
        if resp.status().as_u16() != 200 {
            bail!("Query failed: {}", resp.text()?);
        }
        Ok(ClickhouseRows::new(resp.text()?))
    }

    pub fn select_row(&self, query: &str) -> R<ClickhouseRow> {
        Ok(self
            .select(query)?
            .next()
            .ok_or_else(|| format_err!("no rows returned"))?)
    }

    pub fn insert(&self, query: &str) -> ClickhouseInsert {
        ClickhouseInsert::new(self.url.clone(), query)
    }
}

pub enum Value {
    I64(i64),
    U64(u64),
    F32(f32),
    String(Cow<'static, str>),
    OptionString(Option<Cow<'static, str>>),
    VecString(Vec<Cow<'static, str>>),
}

impl Value {
    pub fn write(&self, w: &mut impl Write) -> R<()> {
        match self {
            Value::I64(v) => write!(w, "{}", v)?,
            Value::U64(v) => write!(w, "{}", v)?,
            Value::F32(v) => write!(w, "{}", v)?,
            Value::String(v) => write!(w, "'{}'", escape_string(v))?,
            Value::OptionString(v) => match v {
                Some(s) => write!(w, "'{}'", escape_string(s))?,
                _ => write!(w, "NULL")?,
            },
            Value::VecString(v) => {
                write!(w, "[")?;
                for (val_idx, val) in v.iter().enumerate() {
                    if val_idx != 0 {
                        write!(w, ",")?;
                    }
                    write!(w, "'{}'", escape_string(val))?;
                }
                write!(w, "]")?;
            }
        }
        Ok(())
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::F32(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I64(v as i64)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::U64(v)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::U64(v as u64)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v.into())
    }
}

impl From<&'static str> for Value {
    fn from(v: &'static str) -> Self {
        Value::String(v.into())
    }
}

impl From<Option<String>> for Value {
    fn from(v: Option<String>) -> Self {
        Value::OptionString(v.map(Into::into))
    }
}

impl From<Option<&'static str>> for Value {
    fn from(v: Option<&'static str>) -> Self {
        Value::OptionString(v.map(Into::into))
    }
}

impl From<Vec<&'static str>> for Value {
    fn from(v: Vec<&'static str>) -> Self {
        Value::VecString(v.into_iter().map(Into::into).collect())
    }
}

impl From<Vec<String>> for Value {
    fn from(v: Vec<String>) -> Self {
        Value::VecString(v.into_iter().map(Into::into).collect())
    }
}

pub struct ClickhouseInsert {
    url: String,
    query: String,
    columns: Vec<Vec<Value>>,
    rows: String,
    rows_count: usize,
}

pub struct ClickhouseInsertRow(Vec<Value>);

impl ClickhouseInsertRow {
    pub fn render(self, w: &mut impl Write) -> R<()> {
        write!(w, "(")?;
        for (idx, val) in self.0.iter().enumerate() {
            if idx != 0 {
                write!(w, ",")?;
            }
            val.write(w)?;
        }
        write!(w, ")")?;
        Ok(())
    }
}

impl ClickhouseInsert {
    pub fn new(url: String, query: &str) -> Self {
        Self {
            url,
            query: query.to_owned(),
            columns: vec![],
            rows: String::new(),
            rows_count: 0,
        }
    }

    pub fn rows_count(&self) -> usize {
        self.rows_count + self.columns.get(0).map(|c| c.len()).unwrap_or(0)
    }

    pub fn row(mut self, values: impl Into<ClickhouseInsertRow>) -> R<Self> {
        self.add_row(values)?;
        Ok(self)
    }

    pub fn add_row(&mut self, values: impl Into<ClickhouseInsertRow>) -> R<()> {
        let r = values.into();
        if !self.rows.is_empty() {
            write!(self.rows, ",")?;
        }
        r.render(&mut self.rows)?;
        self.rows_count += 1;
        Ok(())
    }

    pub fn column(mut self, values: Vec<impl Into<Value>>) -> Self {
        self.columns
            .push(values.into_iter().map(Into::into).collect());
        self
    }

    pub fn exec(self) -> R<()> {
        let mut body = String::new();
        write!(body, "{} VALUES ", self.query)?;
        if !self.rows.is_empty() {
            write!(body, "{}", self.rows)?;
        }
        if !self.columns.is_empty() {
            let length = self.columns[0].len();
            for (column_idx, column) in self.columns.iter().enumerate() {
                if column.len() != length {
                    bail!("All columns must bo of the same length, column {} has different length from others", column_idx);
                }
            }
            for row_idx in 0..length {
                if row_idx != 0 || !self.rows.is_empty() {
                    write!(body, ",")?;
                }
                write!(body, "(")?;
                for (column_idx, column) in self.columns.iter().enumerate() {
                    if column_idx != 0 {
                        write!(body, ",")?;
                    }
                    &column[row_idx].write(&mut body)?;
                }
                write!(body, ")\n")?;
            }
        }
        let mut resp = reqwest::Client::new()
            .post(
                &(self.url
                    + "?wait_end_of_query=1&max_query_size=100000000&max_rows_to_read=1000000000"),
            )
            .body(body.into_bytes())
            .send()?;
        if resp.status().as_u16() != 200 {
            bail!("insert failed: {}", resp.text()?);
        }
        Ok(())
    }
}

fn escape_string(s: &str) -> String {
    s.replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\r", "\\r")
        .replace("'", "\\'")
}

pub struct ClickhouseRows {
    rows: Vec<String>,
    index: usize,
}

impl ClickhouseRows {
    pub fn new(response: String) -> Self {
        Self {
            rows: response
                .split('\n')
                .take_while(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

impl Iterator for ClickhouseRows {
    type Item = ClickhouseRow;

    fn next(&mut self) -> Option<Self::Item> {
        self.index += 1;
        self.rows
            .get(self.index - 1)
            .map(|s| ClickhouseRow::new(s.as_ref()))
    }
}

pub struct ClickhouseRow {
    values: Vec<String>,
}

impl ClickhouseRow {
    pub fn new(row: &str) -> Self {
        Self {
            values: row
                .split('\t')
                .map(|s| {
                    s.replace("\\n", "\n")
                        .replace("\\t", "\t")
                        .replace("\\r", "\r")
                })
                .collect(),
        }
    }
}

impl Index<usize> for ClickhouseRow {
    type Output = str;

    fn index(&self, index: usize) -> &Self::Output {
        self.values[index].as_str()
    }
}
