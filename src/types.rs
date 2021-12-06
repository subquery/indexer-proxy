use crate::error::Error;
use warp::Rejection;

pub type Result<T> = std::result::Result<T, Error>;
pub type WebResult<T> = std::result::Result<T, Rejection>;
