use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nuvyro_transfer_core::pairing::PairingQr;
use nuvyro_transfer_core::protocol::limits::{AUTH_TIMEOUT_SECS, HELLO_TIMEOUT_SECS};
use nuvyro_transfer_core::protocol::{
    EncodedFrame, ErrorCode, ErrorScope, Hello, Id, Message, encode_message,
};

use super::{InvitationStore, PairingError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HandshakeRole {
    Initiator,
    Acceptor,
}

/// Connection authentication only; a transfer ID belongs to a later offer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PairContext {
    pub(crate) secure_session_id: Id,
    pub(crate) pair_id: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Start,
    SendingHello,
    AwaitHello,
    AwaitHelloAck,
    SendingHelloAck,
    AwaitAuthenticate,
    SendingAuthenticate,
    AwaitPairResult,
    SendingPairResult(PairContext),
    Paired,
    Failed,
}

enum Credentials {
    Outgoing { qr: PairingQr, name: String },
    Incoming { invitations: Arc<InvitationStore> },
}

#[derive(Debug)]
pub(crate) enum HandshakeAction {
    Send(EncodedFrame),
    Established(PairContext),
}

/// The TLS owner must enforce its verified profile, generation, and cancellation.
/// Confirm each control frame was flushed with `sent` before processing another frame.
pub(crate) struct HandshakeGate {
    session: Id,
    role: HandshakeRole,
    credentials: Option<Credentials>,
    state: State,
    deadline: Instant,
    last_observed: Instant,
}

impl HandshakeGate {
    pub(crate) fn outgoing(
        qr: PairingQr,
        claimed_name: String,
        now: Instant,
    ) -> Result<Self, ErrorCode> {
        validate_name(&claimed_name)?;
        Ok(Self {
            session: qr.secure_session_id(),
            role: HandshakeRole::Initiator,
            credentials: Some(Credentials::Outgoing {
                qr,
                name: claimed_name,
            }),
            state: State::Start,
            deadline: deadline(now, HELLO_TIMEOUT_SECS)?,
            last_observed: now,
        })
    }

    pub(super) fn incoming(
        invitations: Arc<InvitationStore>,
        now: Instant,
    ) -> Result<Self, ErrorCode> {
        Ok(Self {
            session: invitations.session(),
            role: HandshakeRole::Acceptor,
            credentials: Some(Credentials::Incoming { invitations }),
            state: State::AwaitHello,
            deadline: deadline(now, HELLO_TIMEOUT_SECS)?,
            last_observed: now,
        })
    }

    pub(crate) fn initial_message(&mut self, now: Instant) -> Result<HandshakeAction, ErrorCode> {
        self.check_time(now)?;
        if self.state != State::Start {
            return self.fail(ErrorCode::UnexpectedState);
        }
        self.send(
            Message::SessionHello(Hello {
                secure_session_id: self.session,
            }),
            State::SendingHello,
        )
    }

    /// Call only after the complete frame and pending TLS records have been written.
    pub(crate) fn sent(&mut self, now: Instant) -> Result<Option<PairContext>, ErrorCode> {
        self.check_time(now)?;
        match self.state {
            State::SendingHello => self.state = State::AwaitHelloAck,
            State::SendingHelloAck => {
                self.deadline = match deadline(now, AUTH_TIMEOUT_SECS) {
                    Ok(value) => value,
                    Err(error) => return self.fail(error),
                };
                self.state = State::AwaitAuthenticate;
            }
            State::SendingAuthenticate => self.state = State::AwaitPairResult,
            State::SendingPairResult(pair) => {
                self.state = State::Paired;
                self.credentials = None;
                return Ok(Some(pair));
            }
            _ => return self.fail(ErrorCode::UnexpectedState),
        }
        Ok(None)
    }

    pub(crate) fn receive(
        &mut self,
        message: Message<'_>,
        now: Instant,
    ) -> Result<HandshakeAction, ErrorCode> {
        self.check_time(now)?;
        // All errors pass through this outer boundary, including RNG/encoding/auth failures.
        let result = self.receive_inner(message, now);
        match result {
            Ok(action) => Ok(action),
            Err(error) => self.fail(error),
        }
    }

    fn receive_inner(
        &mut self,
        message: Message<'_>,
        now: Instant,
    ) -> Result<HandshakeAction, ErrorCode> {
        match (self.state, message) {
            (State::AwaitHello, Message::SessionHello(hello)) => {
                if hello.secure_session_id != self.session {
                    return Err(ErrorCode::SessionExpired);
                }
                self.send(Message::SessionHelloAck(hello), State::SendingHelloAck)
            }
            (State::AwaitHelloAck, Message::SessionHelloAck(hello)) => {
                if hello.secure_session_id != self.session {
                    return Err(ErrorCode::SessionExpired);
                }
                let Some(Credentials::Outgoing { qr, name }) = self.credentials.as_ref() else {
                    return Err(ErrorCode::UnexpectedState);
                };
                let frame = encode_message(&Message::PairAuthenticate {
                    invitation_id: qr.invitation_id(),
                    token: qr.token().expose_secret(),
                    claimed_sender_name: name,
                })
                .map_err(|error| error.remote_code())?;
                // The outbound frame is now the sole owner of the token needed for sending.
                self.credentials = None;
                self.deadline = deadline(now, AUTH_TIMEOUT_SECS)?;
                self.state = State::SendingAuthenticate;
                Ok(HandshakeAction::Send(frame))
            }
            (
                State::AwaitAuthenticate,
                Message::PairAuthenticate {
                    invitation_id,
                    token,
                    claimed_sender_name,
                },
            ) => {
                validate_name(claimed_sender_name)?;
                let Some(Credentials::Incoming { invitations }) = self.credentials.as_ref() else {
                    return Err(ErrorCode::UnexpectedState);
                };
                let pair_id = invitations
                    .authenticate(invitation_id, token, now)
                    .map_err(PairingError::remote_code)?;
                let pair = PairContext {
                    secure_session_id: self.session,
                    pair_id,
                };
                self.send(
                    Message::PairResult {
                        pair_id,
                        secure_session_id: self.session,
                    },
                    State::SendingPairResult(pair),
                )
            }
            (
                State::AwaitPairResult,
                Message::PairResult {
                    pair_id,
                    secure_session_id,
                },
            ) => {
                if secure_session_id != self.session {
                    return Err(ErrorCode::SessionExpired);
                }
                self.state = State::Paired;
                Ok(HandshakeAction::Established(PairContext {
                    secure_session_id,
                    pair_id,
                }))
            }
            (
                State::AwaitHello
                | State::AwaitHelloAck
                | State::AwaitAuthenticate
                | State::AwaitPairResult,
                Message::Error {
                    pair_id: None,
                    transfer_id: None,
                    code,
                    scope: ErrorScope::Connection,
                },
            ) => Err(code),
            _ => Err(ErrorCode::UnexpectedState),
        }
    }

    fn send(&mut self, message: Message<'_>, next: State) -> Result<HandshakeAction, ErrorCode> {
        match encode_message(&message) {
            Ok(frame) => {
                self.state = next;
                Ok(HandshakeAction::Send(frame))
            }
            Err(error) => self.fail(error.remote_code()),
        }
    }

    /// The owner calls this on each poll, even if the peer sends no bytes.
    pub(crate) fn check_time(&mut self, now: Instant) -> Result<(), ErrorCode> {
        if matches!(self.state, State::Paired | State::Failed) {
            return self.fail(ErrorCode::UnexpectedState);
        }
        if let Some(Credentials::Incoming { invitations }) = self.credentials.as_ref() {
            if !invitations.is_open() {
                return self.fail(ErrorCode::AuthenticationFailed);
            }
        }
        if now < self.last_observed || now >= self.deadline {
            return self.fail(ErrorCode::DeadlineExceeded);
        }
        self.last_observed = now;
        Ok(())
    }

    pub(crate) fn abort(&mut self) {
        self.credentials = None;
        self.state = State::Failed;
    }

    pub(crate) const fn deadline(&self) -> Instant {
        self.deadline
    }

    fn fail<T>(&mut self, error: ErrorCode) -> Result<T, ErrorCode> {
        self.abort();
        Err(error)
    }
}

impl fmt::Debug for HandshakeGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandshakeGate")
            .field("role", &self.role)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

fn validate_name(name: &str) -> Result<(), ErrorCode> {
    if name.is_empty()
        || name.len() > 128
        || name.chars().count() > 32
        || name.chars().any(char::is_control)
    {
        Err(ErrorCode::MalformedFrame)
    } else {
        Ok(())
    }
}

fn deadline(now: Instant, seconds: u64) -> Result<Instant, ErrorCode> {
    now.checked_add(Duration::from_secs(seconds))
        .ok_or(ErrorCode::DeadlineExceeded)
}

#[cfg(test)]
mod tests;
