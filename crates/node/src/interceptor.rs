use tonic::{Request, Status};

#[derive(Clone)]
pub struct AuthInterceptor {
    expected: String,
}

impl AuthInterceptor {
    pub fn new(token: &str) -> Self {
        Self { expected: format!("Bearer {}", token) }
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, req: Request<()>) -> Result<Request<()>, Status> {
        let provided = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if provided == self.expected {
            Ok(req)
        } else {
            Err(Status::unauthenticated("invalid or missing token"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;
    use tonic::service::Interceptor;

    fn req_with_token(token: &str) -> Request<()> {
        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(token).unwrap(),
        );
        req
    }

    #[test]
    fn valid_token_passes() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("Bearer secret-token");
        assert!(interceptor.call(req).is_ok());
    }

    #[test]
    fn invalid_token_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("Bearer wrong-token");
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn missing_token_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = Request::new(());
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn token_without_bearer_prefix_rejected() {
        let mut interceptor = AuthInterceptor::new("secret-token");
        let req = req_with_token("secret-token");
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}
