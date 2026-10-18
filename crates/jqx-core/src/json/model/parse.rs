//! Helper functions to materialize in-memory `Value` AST from an `EventSource`.
//!
//! Reconstructs subtrees on demand during streaming query execution.

use crate::json::error::{JsonError, JsonErrorKind};
use crate::json::model::{Object, Value};
use crate::json::stream::event::{Event, EventSource, Scalar};

impl<'a> Value<'a> {
    /// Materializes an in-memory `Value` AST from an active `EventSource`.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the event stream terminates unexpectedly
    /// or contains syntactically invalid sequences.
    pub fn parse_from_source<S: EventSource<'a>>(source: &mut S) -> Result<Option<Value<'a>>, JsonError> {
        let Some(first_event) = source.next_event()? else {
            return Ok(None);
        };
        Self::from_event_stream(first_event, source).map(Some)
    }

    fn from_event_stream<S: EventSource<'a>>(
        current: Event<'a>,
        source: &mut S,
    ) -> Result<Value<'a>, JsonError> {
        match current {
            Event::Scalar(Scalar::Null) => Ok(Value::Null),
            Event::Scalar(Scalar::Bool(b)) => Ok(Value::Bool(b)),
            Event::Scalar(Scalar::Number(n)) => Ok(Value::Number(n)),
            Event::Scalar(Scalar::String(s)) => Ok(Value::String(s)),
            Event::StartArray => {
                let mut items = Vec::new();
                loop {
                    let ev = source.next_event()?.ok_or_else(|| {
                        JsonError::new(JsonErrorKind::UnexpectedEof, source.offset())
                    })?;
                    match ev {
                        Event::EndArray => break,
                        other => {
                            let val = Self::from_event_stream(other, source)?;
                            items.push(val);
                        }
                    }
                }
                Ok(Value::Array(items))
            }
            Event::StartObject => {
                let mut obj = Object::new();
                loop {
                    let ev = source.next_event()?.ok_or_else(|| {
                        JsonError::new(JsonErrorKind::UnexpectedEof, source.offset())
                    })?;
                    match ev {
                        Event::EndObject => break,
                        Event::Key(key) => {
                            let val_event = source.next_event()?.ok_or_else(|| {
                                JsonError::new(JsonErrorKind::UnexpectedEof, source.offset())
                            })?;
                            let val = Self::from_event_stream(val_event, source)?;
                            obj.insert(key, val);
                        }
                        _ => {
                            return Err(JsonError::syntax(
                                "expected object key or '}'",
                                source.offset(),
                            ))
                        }
                    }
                }
                Ok(Value::Object(obj))
            }
            Event::EndArray | Event::EndObject | Event::Key(_) => Err(JsonError::syntax(
                "unexpected structural token while parsing value",
                source.offset(),
            )),
        }
    }

    /// Parses a JSON string directly into a `Value` using a streaming `Tokenizer`.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the input is not valid JSON according to RFC 8259.
    pub fn from_json_str(input: &'a str) -> Result<Value<'a>, JsonError>
    where
        'a: 'static,
    {
        let mut tokenizer = crate::json::stream::Tokenizer::new(input.as_bytes());
        match Self::parse_from_source(&mut tokenizer)? {
            Some(v) => Ok(v),
            None => Err(JsonError::new(JsonErrorKind::UnexpectedEof, 0)),
        }
    }
}
