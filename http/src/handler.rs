use Rpc;

use std::{fmt, mem, str};
use std::sync::Arc;

use hyper::{self, service::Service, Body, Method};
use hyper::header::{self, HeaderMap, HeaderValue};

use jsonrpc::{self as core, FutureResult, Metadata, Middleware, NoopMiddleware};
use jsonrpc::futures::{Future, Poll, Async, Stream, future};
use jsonrpc::serde_json;
use response::Response;
use server_utils::cors;
use server_utils::cors::AllowHeaders;

use {utils, RequestMiddleware, RequestMiddlewareAction, CorsDomains, AllowedHosts, RestApi};

/// jsonrpc http request handler.
pub struct ServerHandler<M: Metadata = (), S: Middleware<M> = NoopMiddleware> {
	jsonrpc_handler: Rpc<M, S>,
	allowed_hosts: AllowedHosts,
	cors_domains: CorsDomains,
	cors_max_age: Option<u32>,
	allowed_headers: cors::AccessControlAllowHeadersUnicase,
	middleware: Arc<RequestMiddleware>,
	rest_api: RestApi,
	health_api: Option<(String, String)>,
	max_request_body_size: usize,
}

impl<M: Metadata, S: Middleware<M>> ServerHandler<M, S> {
	/// Create new request handler.
	pub fn new(
		jsonrpc_handler: Rpc<M, S>,
		cors_domains: CorsDomains,
		cors_max_age: Option<u32>,
		allowed_headers: cors::AccessControlAllowHeadersUnicase,
		allowed_hosts: AllowedHosts,
		middleware: Arc<RequestMiddleware>,
		rest_api: RestApi,
		health_api: Option<(String, String)>,
		max_request_body_size: usize,
	) -> Self {
		ServerHandler {
			jsonrpc_handler,
			allowed_hosts,
			cors_domains,
			cors_max_age,
			allowed_headers,
			middleware,
			rest_api,
			health_api,
			max_request_body_size,
		}
	}
}

impl<M: Metadata, S: Middleware<M>> Service for ServerHandler<M, S> {
	type ReqBody = Body;
	type ResBody = Body;
	type Error = hyper::Error;
	type Future = Handler<M, S>;

	fn call(&mut self, request: hyper::Request<Self::ReqBody>) -> Self::Future {
		let is_host_allowed = utils::is_host_allowed(&request, &self.allowed_hosts);
		let action = self.middleware.on_request(request);

		let (should_validate_hosts, should_continue_on_invalid_cors, response) = match action {
			RequestMiddlewareAction::Proceed { should_continue_on_invalid_cors, request }=> (
				true, should_continue_on_invalid_cors, Err(request)
			),
			RequestMiddlewareAction::Respond { should_validate_hosts, response } => (
				should_validate_hosts, false, Ok(response)
			),
		};

		// Validate host
		if should_validate_hosts && !is_host_allowed {
			return Handler::Error(Some(Response::host_not_allowed()));
		}

		// Replace response with the one returned by middleware.
		match response {
			Ok(response) => Handler::Middleware(response),
			Err(request) => {
				Handler::Rpc(RpcHandler {
					jsonrpc_handler: self.jsonrpc_handler.clone(),
					state: RpcHandlerState::ReadingHeaders {
						request: request,
						cors_domains: self.cors_domains.clone(),
						continue_on_invalid_cors: should_continue_on_invalid_cors,
					},
					is_options: false,
					cors_allow_origin: cors::AllowOrigin::NotRequired,
					rest_api: self.rest_api,
					health_api: self.health_api.clone(),
					cors_max_age: self.cors_max_age,
					allowed_headers: self.allowed_headers.clone(),
					cors_allow_headers: cors::AllowHeaders::NotRequired,
					max_request_body_size: self.max_request_body_size,
				})
			}
		}
	}
}

pub enum Handler<M: Metadata, S: Middleware<M>> {
	Rpc(RpcHandler<M, S>),
	Error(Option<Response>),
	Middleware(Box<Future<Item = hyper::Response<Body>, Error = hyper::Error> + Send>),
}

impl<M: Metadata, S: Middleware<M>> Future for Handler<M, S> {
	type Item = hyper::Response<Body>;
	type Error = hyper::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		match *self {
			Handler::Rpc(ref mut handler) => handler.poll(),
			Handler::Middleware(ref mut middleware) => middleware.poll(),
			Handler::Error(ref mut response) => Ok(Async::Ready(
				response.take().expect("Response always Some initialy. Returning `Ready` so will never be polled again; qed").into()
			)),
		}
	}
}

enum RpcPollState<M, F> where
	F: Future<Item = Option<core::Response>, Error = ()>,
{
	Ready(RpcHandlerState<M, F>),
	NotReady(RpcHandlerState<M, F>),
}

impl<M, F> RpcPollState<M, F> where
	F: Future<Item = Option<core::Response>, Error = ()>,
{
	fn decompose(self) -> (RpcHandlerState<M, F>, bool) {
		use self::RpcPollState::*;
		match self {
			Ready(handler) => (handler, true),
			NotReady(handler) => (handler, false),
		}
	}
}

type FutureResponse<F> = future::Map<
	future::Either<future::FutureResult<Option<core::Response>, ()>, core::FutureRpcResult<F>>,
	fn(Option<core::Response>) -> Response,
>;


enum RpcHandlerState<M, F> where
	F: Future<Item = Option<core::Response>, Error = ()>,
{
	ReadingHeaders {
		request: hyper::Request<Body>,
		cors_domains: CorsDomains,
		continue_on_invalid_cors: bool,
	},
	ReadingBody {
		body: hyper::Body,
		uri: Option<hyper::Uri>,
		request: Vec<u8>,
		metadata: M,
	},
	ProcessRest {
		uri: hyper::Uri,
		metadata: M,
	},
	ProcessHealth {
		method: String,
		metadata: M,
	},
	Writing(Response),
	WaitingForResponse(FutureResponse<F>),
	Waiting(FutureResult<F>),
	Done,
}

impl<M, F> fmt::Debug for RpcHandlerState<M, F> where
	F: Future<Item = Option<core::Response>, Error = ()>,
{
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		use self::RpcHandlerState::*;

		match *self {
			ReadingHeaders {..} => write!(fmt, "ReadingHeaders"),
			ReadingBody {..} => write!(fmt, "ReadingBody"),
			ProcessRest {..} => write!(fmt, "ProcessRest"),
			ProcessHealth {..} => write!(fmt, "ProcessHealth"),
			Writing(ref res) => write!(fmt, "Writing({:?})", res),
			WaitingForResponse(_) => write!(fmt, "WaitingForResponse"),
			Waiting(_) => write!(fmt, "Waiting"),
			Done => write!(fmt, "Done"),
		}
	}
}

pub struct RpcHandler<M: Metadata, S: Middleware<M>> {
	jsonrpc_handler: Rpc<M, S>,
	state: RpcHandlerState<M, S::Future>,
	is_options: bool,
	cors_allow_origin: cors::AllowOrigin<header::HeaderValue>,
	cors_max_age: Option<u32>,
	cors_allow_headers: cors::AllowHeaders<header::HeaderMap>,
	allowed_headers: cors::AccessControlAllowHeadersUnicase,
	rest_api: RestApi,
	health_api: Option<(String, String)>,
	max_request_body_size: usize,
}

impl<M: Metadata, S: Middleware<M>> Future for RpcHandler<M, S> {
	type Item = hyper::Response<Body>;
	type Error = hyper::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let allowed_headers = self.allowed_headers.clone();

		let new_state = match mem::replace(&mut self.state, RpcHandlerState::Done) {
			RpcHandlerState::ReadingHeaders { request, cors_domains, continue_on_invalid_cors, } => {
				// Read cors header
				self.cors_allow_origin = utils::cors_allow_origin(&request, &cors_domains);
				self.cors_allow_headers = utils::cors_allow_headers(&request, &allowed_headers);
				self.is_options = *request.method() == Method::OPTIONS;
				// Read other headers
				RpcPollState::Ready(self.read_headers(request, continue_on_invalid_cors))
			},
			RpcHandlerState::ReadingBody { body, request, metadata, uri, } => {
				match self.process_body(body, request, uri, metadata) {
					Err(BodyError::Utf8(ref e)) => {
						let mesg = format!("utf-8 encoding error at byte {} in request body", e.valid_up_to());
						let resp = Response::bad_request(mesg);
						RpcPollState::Ready(RpcHandlerState::Writing(resp))
					}
					Err(BodyError::TooLarge) => {
						let resp = Response::too_large("request body size exceeds allowed maximum");
						RpcPollState::Ready(RpcHandlerState::Writing(resp))
					}
					Err(BodyError::Hyper(e)) => return Err(e),
					Ok(state) => state,
				}
			},
			RpcHandlerState::ProcessRest { uri, metadata } => {
				self.process_rest(uri, metadata)?
			},
			RpcHandlerState::ProcessHealth { method, metadata } => {
				self.process_health(method, metadata)?
			},
			RpcHandlerState::WaitingForResponse(mut waiting) => {
				match waiting.poll() {
					Ok(Async::Ready(response)) => RpcPollState::Ready(RpcHandlerState::Writing(response.into())),
					Ok(Async::NotReady) => RpcPollState::NotReady(RpcHandlerState::WaitingForResponse(waiting)),
					Err(e) => RpcPollState::Ready(RpcHandlerState::Writing(
						Response::internal_error(format!("{:?}", e))
					)),
				}
			},
			RpcHandlerState::Waiting(mut waiting) => {
				match waiting.poll() {
					Ok(Async::Ready(response)) => {
						RpcPollState::Ready(RpcHandlerState::Writing(match response {
							// Notification, just return empty response.
							None => Response::ok(String::new()),
							// Add new line to have nice output when using CLI clients (curl)
							Some(result) => Response::ok(format!("{}\n", result)),
						}.into()))
					},
					Ok(Async::NotReady) => RpcPollState::NotReady(RpcHandlerState::Waiting(waiting)),
					Err(e) => RpcPollState::Ready(RpcHandlerState::Writing(
						Response::internal_error(format!("{:?}", e))
					)),
				}
			},
			state => RpcPollState::NotReady(state),
		};

		let (new_state, is_ready) = new_state.decompose();
		match new_state {
			RpcHandlerState::Writing(res) => {
				let mut response: hyper::Response<Body> = res.into();
				let cors_allow_origin = mem::replace(&mut self.cors_allow_origin, cors::AllowOrigin::Invalid);
				let cors_allow_headers = mem::replace(&mut self.cors_allow_headers, cors::AllowHeaders::Invalid);
				Self::set_response_headers(
					response.headers_mut(),
					self.is_options,
					cors_allow_origin.into(),
					self.cors_max_age,
					cors_allow_headers,
				);
				Ok(Async::Ready(response))
			},
			state => {
				self.state = state;
				if is_ready {
					self.poll()
				} else {
					Ok(Async::NotReady)
				}
			},
		}
	}
}

// Intermediate and internal error type to better distinguish
// error cases occurring during request body processing.
enum BodyError {
	Hyper(hyper::Error),
	Utf8(str::Utf8Error),
	TooLarge,
}

impl From<hyper::Error> for BodyError {
	fn from(e: hyper::Error) -> BodyError {
		BodyError::Hyper(e)
	}
}

impl<M: Metadata, S: Middleware<M>> RpcHandler<M, S> {
	fn read_headers(
		&self,
		request: hyper::Request<Body>,
		continue_on_invalid_cors: bool,
	) -> RpcHandlerState<M, S::Future> {
		if self.cors_allow_origin == cors::AllowOrigin::Invalid && !continue_on_invalid_cors {
			return RpcHandlerState::Writing(Response::invalid_allow_origin());
		}
		if self.cors_allow_headers == cors::AllowHeaders::Invalid && !continue_on_invalid_cors {
			return RpcHandlerState::Writing(Response::invalid_allow_headers());
		}

		// Read metadata
		let metadata = self.jsonrpc_handler.extractor.read_metadata(&request);

		// Proceed
		match request.method().clone() {
			// Validate the ContentType header
			// to prevent Cross-Origin XHRs with text/plain
			Method::POST if Self::is_json(request.headers().get("content-type")) => {
				let uri = if self.rest_api != RestApi::Disabled { Some(request.uri().clone()) } else { None };
				RpcHandlerState::ReadingBody {
					metadata,
					request: Default::default(),
					uri,
					body: request.into_body(),
				}
			},
			Method::POST if self.rest_api == RestApi::Unsecure && request.uri().path().split('/').count() > 2 => {
				RpcHandlerState::ProcessRest {
					metadata,
					uri: request.uri().clone(),
				}
			},
			// Just return error for unsupported content type
			Method::POST => {
				RpcHandlerState::Writing(Response::unsupported_content_type())
			},
			// Don't validate content type on options
			Method::OPTIONS => {
				RpcHandlerState::Writing(Response::empty())
			},
			// Respond to health API request if there is one configured.
			Method::GET if self.health_api.as_ref().map(|x| &*x.0) == Some(request.uri().path()) => {
				RpcHandlerState::ProcessHealth {
					metadata,
					method: self.health_api.as_ref()
							.map(|x| x.1.clone())
							.expect("Health api is defined since the URI matched."),
				}
			},
			// Disallow other methods.
			_ => {
				RpcHandlerState::Writing(Response::method_not_allowed())
			},
		}
	}

	fn process_health(
		&self,
		method: String,
		metadata: M,
	) -> Result<RpcPollState<M, S::Future>, hyper::Error> {
		use self::core::types::{Call, MethodCall, Version, Params, Request, Id, Output, Success, Failure};

		// Create a request
		let call = Request::Single(Call::MethodCall(MethodCall {
			jsonrpc: Some(Version::V2),
			method,
			params: Params::None,
			id: Id::Num(1),
		}));

		return Ok(RpcPollState::Ready(RpcHandlerState::WaitingForResponse(
			future::Either::B(self.jsonrpc_handler.handler.handle_rpc_request(call, metadata))
				.map(|res| match res {
					Some(core::Response::Single(Output::Success(Success { result, .. }))) => {
						let result = serde_json::to_string(&result)
							.expect("Serialization of result is infallible;qed");

						Response::ok(result)
					},
					Some(core::Response::Single(Output::Failure(Failure { error, .. }))) => {
						let result = serde_json::to_string(&error)
							.expect("Serialization of error is infallible;qed");

						Response::service_unavailable(result)
					},
					e => Response::internal_error(format!("Invalid response for health request: {:?}", e)),
				})
		)));
	}

	fn process_rest(
		&self,
		uri: hyper::Uri,
		metadata: M,
	) -> Result<RpcPollState<M, S::Future>, hyper::Error> {
		use self::core::types::{Call, MethodCall, Version, Params, Request, Id, Value};

		// skip the initial /
		let mut it = uri.path().split('/').skip(1);

		// parse method & params
		let method = it.next().unwrap_or("");
		let mut params = Vec::new();
		for param in it {
			let v = serde_json::from_str(param)
				.or_else(|_| serde_json::from_str(&format!("\"{}\"", param)))
				.unwrap_or(Value::Null);
			params.push(v)
		}

		// Create a request
		let call = Request::Single(Call::MethodCall(MethodCall {
			jsonrpc: Some(Version::V2),
			method: method.into(),
			params: Params::Array(params),
			id: Id::Num(1),
		}));

		return Ok(RpcPollState::Ready(RpcHandlerState::Waiting(
			future::Either::B(self.jsonrpc_handler.handler.handle_rpc_request(call, metadata))
				.map(|res| res.map(|x| serde_json::to_string(&x)
					.expect("Serialization of response is infallible;qed")
				))
		)));
	}

	fn process_body(
		&self,
		mut body: hyper::Body,
		mut request: Vec<u8>,
		uri: Option<hyper::Uri>,
		metadata: M,
	) -> Result<RpcPollState<M, S::Future>, BodyError> {
		loop {
			match body.poll()? {
				Async::Ready(Some(chunk)) => {
					if request.len().checked_add(chunk.len()).map(|n| n > self.max_request_body_size).unwrap_or(true) {
						return Err(BodyError::TooLarge)
					}
					request.extend_from_slice(&*chunk)
				},
				Async::Ready(None) => {
					if let (Some(uri), true) = (uri, request.is_empty()) {
						return Ok(RpcPollState::Ready(RpcHandlerState::ProcessRest {
							uri,
							metadata,
						}));
					}

					let content = match str::from_utf8(&request) {
						Ok(content) => content,
						Err(err) => {
							// Return utf error.
							return Err(BodyError::Utf8(err));
						},
					};

					// Content is ready
					return Ok(RpcPollState::Ready(RpcHandlerState::Waiting(
						self.jsonrpc_handler.handler.handle_request(content, metadata)
					)));
				},
				Async::NotReady => {
					return Ok(RpcPollState::NotReady(RpcHandlerState::ReadingBody {
						body,
						request,
						metadata,
						uri,
					}));
				},
			}
		}
	}

	fn set_response_headers(
		headers: &mut HeaderMap,
		is_options: bool,
		cors_allow_origin: Option<HeaderValue>,
		cors_max_age: Option<u32>,
		cors_allow_headers: AllowHeaders<header::HeaderMap>,
	) {
		if is_options {
			headers.append(header::ALLOW, Method::OPTIONS.as_str().parse().expect("`Method` will always parse; qed"));
			headers.append(header::ALLOW, Method::POST.as_str().parse().expect("`Method` will always parse; qed"));

			headers.append(header::ACCEPT, HeaderValue::from_static("application/json"));

		}

		if let Some(cors_domain) = cors_allow_origin {
			headers.append(header::ACCESS_CONTROL_ALLOW_METHODS, Method::OPTIONS.as_str().parse()
				.expect("`Method` will always parse; qed"));
			headers.append(header::ACCESS_CONTROL_ALLOW_METHODS, Method::POST.as_str().parse()
				.expect("`Method` will always parse; qed"));

			if let AllowHeaders::Ok(cors_allow_headers) = cors_allow_headers {
				headers.extend(cors_allow_headers);
			}

			if let Some(cma) = cors_max_age {
				headers.append(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_str(&cma.to_string())
					.expect("`u32` will always parse; qed"));
			}

			headers.append(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_domain);
			headers.append(header::VARY, HeaderValue::from_static("origin"));
		}
	}

	/// Returns true if the `content_type` header indicates a valid JSON
	/// message.
	fn is_json(content_type: Option<&header::HeaderValue>) -> bool {
		match content_type {
			Some(header_val) => {
				match header_val.to_str() {
					Ok(header_str) => {
						header_str == "application/json" || header_str == "application/json; charset=utf-8"
					},
					Err(_) => false,
				}
			}
			_ => false,
		}
	}
}
