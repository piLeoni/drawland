#![doc = include_str!("../README.md")]

use std::sync::{Arc, Mutex};
use swc_common::errors::{DiagnosticBuilder, Emitter, Handler};
use swc_common::sync::Lrc;
use swc_common::{BytePos, SourceMap, GLOBALS};

/// What is wrong with the TypeScript, and where: `(line, column)`, both from 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub message: String,
    pub at: Option<(u32, u32)>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.at {
            Some((l, c)) => write!(f, "line {l} column {c}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone)]
struct Collect(Arc<Mutex<Vec<(String, Option<BytePos>)>>>);

impl Emitter for Collect {
    fn emit(&mut self, db: &mut DiagnosticBuilder<'_>) {
        let at = db.span.primary_span().map(|s| s.lo);
        self.0.lock().expect("one thread").push((db.message(), at));
    }
}

/// `source` without its types.
pub fn strip_types(source: &str) -> Result<String, Error> {
    let found = Collect(Arc::new(Mutex::new(Vec::new())));
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_emitter(true, false, Box::new(found.clone()));
    let options = swc_ts_fast_strip::Options { filename: Some("profile.ts".into()), ..Default::default() };
    let result = GLOBALS.set(&Default::default(), || swc_ts_fast_strip::operate(&cm, &handler, source.to_string(), options));
    let first = found.0.lock().expect("one thread").first().cloned();
    match (result, first) {
        (Ok(out), None) => Ok(out.code),
        (result, first) => {
            let (message, at) = match first {
                Some((m, Some(pos))) => {
                    let loc = cm.lookup_char_pos(pos);
                    (m, Some((loc.line as u32, loc.col.0 as u32 + 1)))
                }
                Some((m, None)) => (m, None),
                None => (result.err().map(|e| e.message).unwrap_or_default(), None),
            };
            Err(Error { message: format!("TypeScript: {message}"), at })
        }
    }
}

