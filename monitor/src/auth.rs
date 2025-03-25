use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

use crate::DOTENV;

pub struct ApiKeyGuard<'req>(/* only FromRequest can construct */ &'req str);

#[rocket::async_trait]
impl<'req> FromRequest<'req> for ApiKeyGuard<'req> {
    type Error = ();

    async fn from_request(req: &'req Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Authorization") {
            None => Outcome::Error((Status::Unauthorized, ())),
            Some(value) => {
                if value == DOTENV.monitor_api_token_authorization_value {
                    Outcome::Success(ApiKeyGuard(value))
                } else {
                    Outcome::Error((Status::Forbidden, ()))
                }
            }
        }
    }
}
