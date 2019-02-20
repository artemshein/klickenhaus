#!/usr/bin/env run-cargo-script
//! ```cargo
//! [package]
//! edition = "2018"
//! [dependencies]
//! itertools = "0.8"
//! ```

use std::env;
use std::path::PathBuf;
use itertools::join;
use std::fs::File;
use std::fmt::Write;

fn main() {
    let mut path = PathBuf::from(env::var("CARGO_SCRIPT_SCRIPT_PATH").unwrap());
    path.pop();
    path.pop();
    path.push("src/generated.rs");
    let mut s = String::new();

    write!(&mut s, "use crate::*;\n\n").unwrap();

    for i in 1..=32 {
        write!(&mut s, "impl<{types},> From<({tuple_types},)> for ClickhouseInsertRow {{
    fn from(v: ({tuple_types},)) -> ClickhouseInsertRow {{
        ClickhouseInsertRow(vec![{vec}])
    }}
}}\n",
               types = join((1..=i).map(|i| format!("T{}: Into<Value>", i)), ", "),
               tuple_types = join((1..=i).map(|i| format!("T{}", i)), ", "),
               vec = join((1..=i).map(|i| format!("v.{}.into()", i - 1)), ", ")
        ).unwrap();
    }

    use std::io::Write;

    File::create(path).unwrap().write_all(s.as_bytes()).unwrap();
}
