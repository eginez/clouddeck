//! Requests and responses crossing `clouddeck_call` are MessagePack in the map form with field
//! names, each buffer prefixed by one format byte. Unknown and missing fields are tolerated by
//! serde defaults, so `skip_serializing_if`, `flatten` and `untagged` all work across the boundary.
//!
//! Changing the wire format is a boundary break and must bump API_VERSION in lib.rs.

use std::time::{Duration, Instant};

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, Call, ConnectionId, ErrorKind, FlowId, Host, Instance,
    KindId, Provider, ProviderError, ResourceId, Selection,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Extension;

const MSGPACK: u8 = 1;

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = vec![MSGPACK];
    rmp_serde::encode::write_named(&mut bytes, value).map_err(|error| error.to_string())?;
    Ok(bytes)
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    match bytes.split_first() {
        Some((&MSGPACK, body)) => rmp_serde::from_slice(body).map_err(|error| error.to_string()),
        Some((format, _)) => Err(format!(
            "boundary format {format} is not supported; rebuild the extension for this CloudDeck"
        )),
        None => Err("empty message".into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Method {
    Info = 1,
    Connections,
    Dimensions,
    Kinds,
    List,
    Describe,
    AuthMethods,
    BeginAuth,
    ContinueAuth,
    AuthStatus,
    SignOut,
    ConsoleUrl,
    Fields,
    Inspector,
    Identity,
    CheckAccess,
    Settings,
    SettingsChanged,
}

const METHODS: [Method; 18] = [
    Method::Info,
    Method::Connections,
    Method::Dimensions,
    Method::Kinds,
    Method::List,
    Method::Describe,
    Method::AuthMethods,
    Method::BeginAuth,
    Method::ContinueAuth,
    Method::AuthStatus,
    Method::SignOut,
    Method::ConsoleUrl,
    Method::Fields,
    Method::Inspector,
    Method::Identity,
    Method::CheckAccess,
    Method::Settings,
    Method::SettingsChanged,
];

impl Method {
    pub fn from_code(code: u32) -> Option<Self> {
        METHODS.into_iter().find(|method| *method as u32 == code)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request<A> {
    pub provider: String,
    /// Milliseconds left before the host stops waiting; the extension rebuilds its deadline.
    pub deadline_ms: Option<u64>,
    pub args: A,
}

pub fn dispatch<E: Extension>(
    extension: &E,
    host: &dyn Host,
    method: u32,
    request: &[u8],
) -> Vec<u8> {
    let unsupported = || error_response(ErrorKind::Unsupported, format!("unknown method {method}"));
    let Some(method) = Method::from_code(method) else {
        return unsupported();
    };
    catalog_call(extension, host, method, request)
        .or_else(|| auth_call(extension, host, method, request))
        .unwrap_or_else(unsupported)
}

fn catalog_call<E: Extension>(
    extension: &E,
    host: &dyn Host,
    method: Method,
    request: &[u8],
) -> Option<Vec<u8>> {
    let response = match method {
        Method::Info => serve(extension, host, request, |p, _, ()| Ok(p.info())),
        Method::Connections => serve(extension, host, request, |p, cx, ()| p.connections(cx)),
        Method::Dimensions => serve(
            extension,
            host,
            request,
            |p, cx, connection: ConnectionId| p.dimensions(cx, &connection),
        ),
        Method::Kinds => serve(extension, host, request, |p, _, ()| Ok(p.kinds())),
        Method::List => serve(
            extension,
            host,
            request,
            |p, cx, (connection, kind, selection): (ConnectionId, KindId, Selection)| {
                Ok(p.list(cx, &connection, &kind, &selection))
            },
        ),
        Method::Describe => serve(
            extension,
            host,
            request,
            |p, cx, (connection, kind, id): (ConnectionId, KindId, ResourceId)| {
                p.describe(cx, &connection, &kind, &id)
            },
        ),
        Method::ConsoleUrl => serve(
            extension,
            host,
            request,
            |p, _, (connection, kind, resource): (ConnectionId, KindId, Instance)| {
                Ok(p.console_url(&connection, &kind, &resource))
            },
        ),
        Method::Fields => serve(extension, host, request, |p, _, kind: KindId| {
            Ok(p.fields(&kind))
        }),
        Method::Inspector => serve(extension, host, request, |p, _, kind: KindId| {
            Ok(p.inspector(&kind))
        }),
        Method::Identity => serve(
            extension,
            host,
            request,
            |p, cx, (connection, selection): (ConnectionId, Selection)| {
                p.identity(cx, &connection, &selection)
            },
        ),
        Method::Settings => serve(extension, host, request, |p, _, ()| Ok(p.settings())),
        Method::SettingsChanged => {
            serve(extension, host, request, |p, cx, changed: Vec<String>| {
                p.settings_changed(cx, &changed);
                Ok(())
            })
        }
        _ => return None,
    };
    Some(response)
}

fn auth_call<E: Extension>(
    extension: &E,
    host: &dyn Host,
    method: Method,
    request: &[u8],
) -> Option<Vec<u8>> {
    let response = match method {
        Method::AuthMethods => serve(extension, host, request, |p, _, ()| Ok(p.auth_methods())),
        Method::BeginAuth => serve(
            extension,
            host,
            request,
            |p, cx, (auth_method, input): (AuthMethod, AuthInput)| {
                p.begin_auth(cx, &auth_method, input)
            },
        ),
        Method::ContinueAuth => serve(
            extension,
            host,
            request,
            |p, cx, (flow, answer): (FlowId, AuthAnswer)| p.continue_auth(cx, &flow, answer),
        ),
        Method::AuthStatus => serve(
            extension,
            host,
            request,
            |p, cx, connection: Option<ConnectionId>| Ok(p.auth_status(cx, connection.as_ref())),
        ),
        Method::SignOut => serve(
            extension,
            host,
            request,
            |p, cx, connection: Option<ConnectionId>| p.sign_out(cx, connection.as_ref()),
        ),
        Method::CheckAccess => serve(
            extension,
            host,
            request,
            |p, cx, (connection, kind, selection): (ConnectionId, KindId, Selection)| {
                p.check_access(cx, &connection, &kind, &selection)
            },
        ),
        _ => return None,
    };
    Some(response)
}

fn serve<E, A, R>(
    extension: &E,
    host: &dyn Host,
    request: &[u8],
    handle: impl FnOnce(&dyn Provider, &Call<'_>, A) -> Result<R, ProviderError>,
) -> Vec<u8>
where
    E: Extension,
    A: DeserializeOwned,
    R: Serialize,
{
    let outcome = decode::<Request<A>>(request)
        .map_err(|error| {
            ProviderError::new(ErrorKind::Other, format!("malformed request: {error}"))
        })
        .and_then(|request| {
            let provider = extension.provider(&request.provider).ok_or_else(|| {
                ProviderError::new(
                    ErrorKind::NotFound,
                    format!("no provider {}", request.provider),
                )
            })?;
            let deadline = request
                .deadline_ms
                .map(|ms| {
                    Instant::now()
                        .checked_add(Duration::from_millis(ms))
                        .ok_or_else(|| {
                            ProviderError::new(ErrorKind::Timeout, "deadline out of range")
                        })
                })
                .transpose()?;
            let cx = Call { deadline, host };
            handle(provider.as_ref(), &cx, request.args)
        });
    respond(outcome)
}

fn respond<T: Serialize>(outcome: Result<T, ProviderError>) -> Vec<u8> {
    encode(&outcome).unwrap_or_else(|error| {
        error_response(
            ErrorKind::Other,
            format!("response could not be encoded: {error}"),
        )
    })
}

pub fn error_response(kind: ErrorKind, message: impl Into<String>) -> Vec<u8> {
    encode(&Err::<(), _>(ProviderError::new(kind, message))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_code_round_trips_every_method() {
        for code in 1..=18 {
            let method = Method::from_code(code).expect("method codes 1 to 18 are assigned");
            assert_eq!(method as u32, code);
        }
        assert_eq!(Method::from_code(0), None);
        assert_eq!(Method::from_code(19), None);
    }

    #[test]
    fn buffers_in_another_format_are_refused_with_a_rebuild_message() {
        let json = br#"["a"]"#;
        let error = decode::<Vec<String>>(json).unwrap_err();
        assert!(error.contains("rebuild the extension"), "{error}");
        assert_eq!(decode::<Vec<String>>(&[]).unwrap_err(), "empty message");
    }
}
