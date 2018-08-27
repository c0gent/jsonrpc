use hyper::{self, header};
// use hyperx::header;
// use http;

use server_utils::{cors, hosts};
pub use server_utils::cors::CorsHeader;

/// Extracts string value of a single header in request.
fn read_header<'a>(req: &'a hyper::Request<hyper::Body>, header_name: &str) -> Option<&'a str> {
	println!("###### READ_HEADER: req: {:?}, header_name: {:?}", req, header_name);

	req.headers().get(header_name).and_then(|v| v.to_str().ok())
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
// ) -> CorsHeader<header::HeaderValue> {
) -> CorsHeader<header::HeaderMap> {
	cors::get_cors_header(read_header(request, "origin"), read_header(request, "host"), cors_domains).map(|origin| {
		use self::cors::AccessControlAllowOrigin::*;
		let mut headers = header::HeaderMap::new();
		let val = match origin {
			Value(ref val) => header::HeaderValue::from_str(val).expect("FIXME: Can this be invalid? How to handle? Return result instead?"),
			Null => header::HeaderValue::from_static("null"),
			Any => header::HeaderValue::from_static("*"),
		};
		headers.append(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
		println!("###### CORS_HEADER: {:?}", headers);
		headers
	})
}
