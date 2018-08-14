use hyper::{self, header};
// use hyperx::header;
// use http;

use server_utils::{cors, hosts};
pub use server_utils::cors::CorsHeader;

/// Extracts string value of a single header in request.
fn read_header<'a>(req: &'a hyper::Request<hyper::Body>, header: &str) -> Option<&'a str> {
	match req.headers().get(header) {
		Some(ref v) if v.len() == 1 => {
			// ::std::str::from_utf8(&v[0]).ok()
			// ::std::str::from_utf8(&v.as_bytes()[0..1]).ok()
			v.to_str().ok()
		},
		_ => None
	}
}

/// Returns `true` if Host header in request matches a list of allowed hosts.
pub fn is_host_allowed(
	request: &hyper::Request<hyper::Body>,
	allowed_hosts: &Option<Vec<hosts::Host>>,
) -> bool {
	hosts::is_host_valid(read_header(request, "host"), allowed_hosts)
}

/// Returns a CORS header that should be returned with that request.
pub fn cors_header(
	request: &hyper::Request<hyper::Body>,
	cors_domains: &Option<Vec<cors::AccessControlAllowOrigin>>
) -> CorsHeader<header::HeaderValue> {
	cors::get_cors_header(read_header(request, "origin"), read_header(request, "host"), cors_domains).map(|origin| {
		use self::cors::AccessControlAllowOrigin::*;
		match origin {
			Value(ref val) => header::HeaderValue::from_str(val).expect("FIXME: Can this be invalid?"),
			Null => header::HeaderValue::from_static("null"),
			Any => header::HeaderValue::from_static("*"),
		}
	})
}
