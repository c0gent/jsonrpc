//! Basic Request/Response structures used internally.

pub use hyper::{self, Method, Body, StatusCode, header::HeaderValue};

/// Simple server response structure
#[derive(Debug)]
pub struct Response {
	/// Response code
	pub code: StatusCode,
	/// Response content type
	pub content_type: HeaderValue,
	/// Response body
	pub content: String,
}

impl Response {
	/// Create a response with empty body and 200 OK status code.
	pub fn empty() -> Self {
		Self::ok(String::new())
	}

	/// Create a response with given body and 200 OK status code.
	pub fn ok<T: Into<String>>(response: T) -> Self {
		Response {
			code: StatusCode::OK,
			// content_type: hyper::header::ContentType::json(),
			content_type: HeaderValue::from_static("application/json"),
			content: response.into(),
		}
	}

	/// Create a response for internal error.
	pub fn internal_error() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			// content_type: hyper::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: "Provided Host header is not whitelisted.\n".to_owned(),
		}
	}

	/// Create a response for not allowed hosts.
	pub fn host_not_allowed() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			// content_type: hyper::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: "Provided Host header is not whitelisted.\n".to_owned(),
		}
	}

	/// Create a response for unsupported content type.
	pub fn unsupported_content_type() -> Self {
		Response {
			code: StatusCode::UNSUPPORTED_MEDIA_TYPE,
			// content_type: hyperx::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: "Supplied content type is not allowed. Content-Type: application/json is required\n".to_owned(),
		}
	}

	/// Create a response for disallowed method used.
	pub fn method_not_allowed() -> Self {
		Response {
			code: StatusCode::METHOD_NOT_ALLOWED,
			// content_type: hyperx::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: "Used HTTP Method is not allowed. POST or OPTIONS is required\n".to_owned(),
		}
	}

	/// CORS invalid
	pub fn invalid_cors() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			// content_type: hyperx::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: "Origin of the request is not whitelisted. CORS headers would not be sent and any side-effects were cancelled as well.\n".to_owned(),
		}
	}

	/// Create a response for bad request
	pub fn bad_request<S: Into<String>>(msg: S) -> Self {
		Response {
			code: StatusCode::BAD_REQUEST,
			// content_type: hyperx::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: msg.into()
		}
	}

	/// Create a response for too large (413)
	pub fn too_large<S: Into<String>>(msg: S) -> Self {
		Response {
			code: StatusCode::PAYLOAD_TOO_LARGE,
			// content_type: hyperx::header::ContentType::plaintext(),
			content_type: HeaderValue::from_static("text/plain; charset=utf-8"),
			content: msg.into()
		}
	}
}

// impl Into<hyper::Response<Body>> for Response {
// 	fn into(self) -> hyper::Response<Body> {
// 		hyper::Response::new()
// 			.with_status(self.code)
// 			.with_header(self.content_type)
// 			.with_body(self.content)
// 	}
// }

// TODO: Consider removing this panicking conversion or else switch to a
// `HttpTryFrom` or `TryFrom` conversion.
impl From<Response> for hyper::Response<Body> {
	/// Converts from a jsonrpc `Response` to a `hyper::Response`
	///
	/// ## Panics
	///
	/// Panics if the response cannot be converted.
	///
	fn from(res: Response) -> hyper::Response<Body> {
		hyper::Response::builder()
			.status(res.code)
			.header("content-type", res.content_type)
			.body(res.content.into())
			.expect("Unable to convert response")
	}
}
