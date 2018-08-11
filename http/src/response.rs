//! Basic Request/Response structures used internally.

// use hyper::server;
use http::{self, StatusCode};

pub use hyper::{Method, Body};
pub use hyperx::header;

/// Simple server response structure
#[derive(Debug)]
pub struct Response {
	/// Response code
	pub code: StatusCode,
	/// Response content type
	pub content_type: header::ContentType,
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
			content_type: header::ContentType::json(),
			content: response.into(),
		}
	}

	/// Create a response for internal error.
	pub fn internal_error() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			content_type: header::ContentType::plaintext(),
			content: "Provided Host header is not whitelisted.\n".to_owned(),
		}
	}

	/// Create a response for not allowed hosts.
	pub fn host_not_allowed() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			content_type: header::ContentType::plaintext(),
			content: "Provided Host header is not whitelisted.\n".to_owned(),
		}
	}

	/// Create a response for unsupported content type.
	pub fn unsupported_content_type() -> Self {
		Response {
			code: StatusCode::UNSUPPORTED_MEDIA_TYPE,
			content_type: header::ContentType::plaintext(),
			content: "Supplied content type is not allowed. Content-Type: application/json is required\n".to_owned(),
		}
	}

	/// Create a response for disallowed method used.
	pub fn method_not_allowed() -> Self {
		Response {
			code: StatusCode::METHOD_NOT_ALLOWED,
			content_type: header::ContentType::plaintext(),
			content: "Used HTTP Method is not allowed. POST or OPTIONS is required\n".to_owned(),
		}
	}

	/// CORS invalid
	pub fn invalid_cors() -> Self {
		Response {
			code: StatusCode::FORBIDDEN,
			content_type: header::ContentType::plaintext(),
			content: "Origin of the request is not whitelisted. CORS headers would not be sent and any side-effects were cancelled as well.\n".to_owned(),
		}
	}

	/// Create a response for bad request
	pub fn bad_request<S: Into<String>>(msg: S) -> Self {
		Response {
			code: StatusCode::BAD_REQUEST,
			content_type: header::ContentType::plaintext(),
			content: msg.into()
		}
	}

	/// Create a response for too large (413)
	pub fn too_large<S: Into<String>>(msg: S) -> Self {
		Response {
			code: StatusCode::PAYLOAD_TOO_LARGE,
			content_type: header::ContentType::plaintext(),
			content: msg.into()
		}
	}
}

// impl Into<http::Response<Body>> for Response {
// 	fn into(self) -> http::Response<Body> {
// 		http::Response::new()
// 			.with_status(self.code)
// 			.with_header(self.content_type)
// 			.with_body(self.content)
// 	}
// }

impl From<Response> for http::Response<Body> {
	fn from(res: Response) -> http::Response<Body> {
		let mut headers = http::HeaderMap::with_capacity(1);
		headers.append(res.content_type);
		let parts = http::response::Parts {
			status: res.code,
			version: http::version::Version::HTTP_11,
			headers,
			extensions: http::Extensions::new(),
		};
		http::Response::from_parts(parts, res.content)
	}
}
