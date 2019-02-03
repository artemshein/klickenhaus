use failure::{Error, bail};
use std::{ops::Index, fmt::Write};


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

    pub fn query(&self, query: &str) -> Result<(), Error> {
        let mut resp = reqwest::Client::new().post(&self.url).body(query.as_bytes().to_owned()).send()?;
        if resp.status().as_u16() != 200 {
            bail!("Query failed: {}", resp.text()?);
        }
        Ok(())
    }

    pub fn select(&self, query: &str) -> Result<ClickhouseRows, Error> {
        let mut resp = reqwest::Client::new().post(&self.url).body(query.as_bytes().to_owned()).send()?;
        if resp.status().as_u16() != 200 {
            bail!("Query failed: {}", resp.text()?);
        }
        Ok(ClickhouseRows::new(resp.text()?))
    }

    pub fn insert(&self, query: &str) -> ClickhouseInsert {
        ClickhouseInsert::new(self.url.clone(), query)
    }
}

pub enum Value {
    I64(i64),
    U64(u64),
    String(String),
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

impl<'a> From<&'a str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

pub struct ClickhouseInsert {
    url: String,
    query: String,
    columns: Vec<Vec<Value>>,
}

impl ClickhouseInsert {
    pub fn new(url: String, query: &str) -> Self {
        Self {
            url,
            query: query.to_owned(),
            columns: vec![],
        }
    }

    pub fn column(mut self, values: Vec<impl Into<Value>>) -> Self {
        self.columns.push(values.into_iter().map(Into::into).collect());
        self
    }

    pub fn exec(self) -> Result<(), Error> {
        let length = self.columns[0].len();
        for (column_idx, column) in self.columns.iter().enumerate() {
            if column.len() != length {
                bail!("All columns must bo of the same length, column {} has different length from others", column_idx);
            }
        }
        let mut body = String::new();
        write!(body, "{} VALUES ", self.query)?;
        for row_idx in 0..self.columns[0].len() {
            if row_idx != 0 {
                write!(body, ",")?;
            }
            write!(body, "(")?;
            for (column_idx, column) in self.columns.iter().enumerate() {
                if column_idx != 0 {
                    write!(body, ",")?;
                }
                match &column[row_idx] {
                    Value::I64(v) => write!(body, "{}", v)?,
                    Value::U64(v) => write!(body, "{}", v)?,
                    Value::String(v) => write!(body, "'{}'", v.replace("\n", "\\n").replace("\r", "\\r").replace("\r", "\\r").replace("'", "\\'"))?,
                };
            }
            write!(body, ")")?;
        }
        let mut resp = reqwest::Client::new().post(&self.url).body(body.into_bytes()).send()?;
        if resp.status().as_u16() != 200 {
            bail!("insert failed: {}", resp.text()?);
        }
        Ok(())
    }
}

pub struct ClickhouseRows {
    rows: Vec<String>,
    index: usize,
}

impl ClickhouseRows {
    pub fn new(response: String) -> Self {
        Self {
            rows: response.split('\n').take_while(|s| !s.is_empty()).map(ToOwned::to_owned).collect(),
            index: 0,
        }
    }
}

impl Iterator for ClickhouseRows {
    type Item = ClickhouseRow;

    fn next(&mut self) -> Option<Self::Item> {
        self.index += 1;
        self.rows.get(self.index - 1).map(|s| ClickhouseRow::new(s.as_ref()))
    }
}

pub struct ClickhouseRow {
    values: Vec<String>,
}

impl ClickhouseRow {
    pub fn new(row: &str) -> Self {
        Self {
            values: row.split('\t')
                .map(|s| s.replace("\\n", "\n").replace("\\t", "\t").replace("\\r", "\r")).collect(),
        }
    }
}

impl Index<usize> for ClickhouseRow {
    type Output = str;

    fn index(&self, index: usize) -> &Self::Output {
        self.values[index].as_str()
    }
}

#[test]
fn test() -> Result<(), Error> {
    let ch = ClickhouseClient::new("http://localhost:8123");
    if !ch.ping() {
        bail!("Ping failed");
    }
    ch.query("DROP TABLE IF EXISTS test_table")?;
    ch.query("CREATE TABLE IF NOT EXISTS test_table (a Int32, b Enum8('a' = 1, 'b' = 2), c String) ENGINE=Log")?;
    ch.query("INSERT INTO test_table (a, b, c) VALUES (10, 'a', 'test'), (20, 'b', 'test2\ttest3\ntest4\rtest5')")?;
    ch.insert("INSERT INTO test_table (a, b, c)")
        .column(vec![50, 60, 70, 80])
        .column(vec!["a", "b", "a", "b"])
        .column(vec!["str1", "str2", "str3", "str4"])
        .exec()?;
    for row in ch.select("SELECT * FROM test_table")? {
        println!("'{}', '{}', '{}'", &row[0], &row[1], &row[2]);
    }
    ch.query("DROP TABLE test_table")?;
    Ok(())
}
