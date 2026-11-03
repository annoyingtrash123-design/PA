//! API errors: a status code and a message, rendered as `{"error": "..."}`.

#[derive(Debug, Clone)]
pub struct ApiErr {
    pub status: u16,
    pub msg: String,
}

pub type Res<T> = Result<T, ApiErr>;

impl ApiErr {
    pub fn bad(msg: impl Into<String>) -> Self {
        ApiErr { status: 400, msg: msg.into() }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiErr { status: 404, msg: msg.into() }
    }
}
