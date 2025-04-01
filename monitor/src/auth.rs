use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr},
};

use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

use crate::DOTENV;

pub struct ApiKeyGuard<'req> {
    /// Only FromRequest can construct this.
    _value: &'req str,
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for ApiKeyGuard<'req> {
    type Error = ();

    async fn from_request(req: &'req Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authorization") {
            None => Outcome::Error((Status::Unauthorized, ())),
            Some(value) => {
                if value == DOTENV.monitor_api_token_authorization_value {
                    Outcome::Success(ApiKeyGuard { _value: value })
                } else {
                    Outcome::Error((Status::Forbidden, ()))
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct RemoteAddr {
    client_ip: Option<IpAddr>,
    real_ip: Option<IpAddr>,
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for RemoteAddr {
    type Error = ();

    async fn from_request(request: &'req Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(Self {
            client_ip: request.client_ip(),
            real_ip: request.real_ip(),
        })
    }
}

impl PartialEq<Ipv4Addr> for RemoteAddr {
    fn eq(&self, other: &Ipv4Addr) -> bool {
        match self.real_ip.or(self.client_ip) {
            Some(IpAddr::V4(ipv4)) => ipv4 == *other,
            Some(IpAddr::V6(ipv6)) => ipv6 == other.to_ipv6_mapped(),
            None => false,
        }
    }
}

impl Display for RemoteAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ip) = self.real_ip.or(self.client_ip) {
            write!(f, "{}", ip);
        }
        Ok(())
    }
}
