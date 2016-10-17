extern crate toml;

use std::path::Path;

use self::toml::Parser;

pub use self::toml::{Table, Value};

use errors::*;
use io;

pub fn parse(file: &Path) -> Result<Value> {
    if let Some(table) = Parser::new(&try!(io::read(file))).parse() {
        Ok(Value::Table(table))
    } else {
        try!(Err(format!("error parsing {} as TOML", file.display())))
    }
}

trait ValueExt {
    fn lookup_string(&self, key: &str) -> Result<&str>;
}

impl ValueExt for Value {
    fn lookup_string(&self, key: &str) -> Result<&str> {
        if let Some(s) = self.lookup(key).and_then(|v| v.as_str()) {
            Ok(s)
        } else {
            try!(Err(format!("key {} not found", key)))
        }
    }
}
