// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! The Arrow Flight gRPC service, served over `tonic`'s transport with our own
//! protobuf codec. Only the calls this platform uses are routed (`Handshake`,
//! `GetFlightInfo`, `GetSchema`, `DoGet`, `DoPut`); anything else gets gRPC
//! `UNIMPLEMENTED`.

use crate::pb::{
    FlightData, FlightDescriptor, FlightInfo, HandshakeRequest, HandshakeResponse, Message,
    PutResult, SchemaResult, Ticket,
};
use bytes::{Buf, BufMut};
use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::task::{Context, Poll};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder, Streaming};
use tonic::codegen::tokio_stream::Stream;
use tonic::codegen::{Body, BoxFuture, Service, StdError, http};
use tonic::{Request, Response, Status};

pub const SERVICE_NAME: &str = "arrow.flight.protocol.FlightService";

const HANDSHAKE: &str = "/arrow.flight.protocol.FlightService/Handshake";
const GET_FLIGHT_INFO: &str = "/arrow.flight.protocol.FlightService/GetFlightInfo";
const GET_SCHEMA: &str = "/arrow.flight.protocol.FlightService/GetSchema";
const DO_GET: &str = "/arrow.flight.protocol.FlightService/DoGet";
const DO_PUT: &str = "/arrow.flight.protocol.FlightService/DoPut";

// ---- codec ----------------------------------------------------------------

/// gRPC codec for our protobuf messages. `E` is what we send, `D` what we receive.
pub struct PbCodec<E, D>(PhantomData<fn() -> (E, D)>);

impl<E, D> Default for PbCodec<E, D> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

pub struct PbEncoder<E>(PhantomData<fn() -> E>);
pub struct PbDecoder<D>(PhantomData<fn() -> D>);

impl<E: Message, D: Message> Codec for PbCodec<E, D> {
    type Encode = E;
    type Decode = D;
    type Encoder = PbEncoder<E>;
    type Decoder = PbDecoder<D>;

    fn encoder(&mut self) -> Self::Encoder {
        PbEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        PbDecoder(PhantomData)
    }
}

impl<E: Message> Encoder for PbEncoder<E> {
    type Item = E;
    type Error = Status;

    fn encode(&mut self, item: E, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        dst.put_slice(&item.encode_to_vec());
        Ok(())
    }
}

impl<D: Message> Decoder for PbDecoder<D> {
    type Item = D;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<D>, Status> {
        let bytes = src.copy_to_bytes(src.remaining());
        D::decode_raw(&bytes)
            .map(Some)
            .map_err(|e| Status::invalid_argument(e.to_string()))
    }
}

// ---- service trait --------------------------------------------------------

#[tonic::async_trait]
pub trait FlightService: Send + Sync + 'static {
    type HandshakeStream: Stream<Item = Result<HandshakeResponse, Status>> + Send + 'static;
    type DoGetStream: Stream<Item = Result<FlightData, Status>> + Send + 'static;
    type DoPutStream: Stream<Item = Result<PutResult, Status>> + Send + 'static;

    async fn handshake(
        &self,
        request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status>;

    async fn get_flight_info(
        &self,
        request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status>;

    async fn get_schema(
        &self,
        request: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status>;

    async fn do_get(&self, request: Request<Ticket>)
    -> Result<Response<Self::DoGetStream>, Status>;

    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status>;
}

// ---- server ---------------------------------------------------------------

struct UnaryFn<F>(F);
struct ServerStreamingFn<F>(F);
struct StreamingFn<F>(F);

impl<F, Fut, Req, Resp> tonic::server::UnaryService<Req> for UnaryFn<F>
where
    F: FnMut(Request<Req>) -> Fut,
    Fut: Future<Output = Result<Response<Resp>, Status>> + Send + 'static,
{
    type Response = Resp;
    type Future = BoxFuture<Response<Resp>, Status>;

    fn call(&mut self, request: Request<Req>) -> Self::Future {
        Box::pin((self.0)(request))
    }
}

impl<F, Fut, Req, Resp, S> tonic::server::ServerStreamingService<Req> for ServerStreamingFn<F>
where
    F: FnMut(Request<Req>) -> Fut,
    Fut: Future<Output = Result<Response<S>, Status>> + Send + 'static,
    S: Stream<Item = Result<Resp, Status>> + Send + 'static,
{
    type Response = Resp;
    type ResponseStream = S;
    type Future = BoxFuture<Response<S>, Status>;

    fn call(&mut self, request: Request<Req>) -> Self::Future {
        Box::pin((self.0)(request))
    }
}

impl<F, Fut, Req, Resp, S> tonic::server::StreamingService<Req> for StreamingFn<F>
where
    F: FnMut(Request<Streaming<Req>>) -> Fut,
    Fut: Future<Output = Result<Response<S>, Status>> + Send + 'static,
    S: Stream<Item = Result<Resp, Status>> + Send + 'static,
{
    type Response = Resp;
    type ResponseStream = S;
    type Future = BoxFuture<Response<S>, Status>;

    fn call(&mut self, request: Request<Streaming<Req>>) -> Self::Future {
        Box::pin((self.0)(request))
    }
}

pub struct FlightServiceServer<T> {
    inner: Arc<T>,
    max_decoding_message_size: Option<usize>,
    max_encoding_message_size: Option<usize>,
}

impl<T> FlightServiceServer<T> {
    pub fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    pub fn from_arc(inner: Arc<T>) -> Self {
        Self {
            inner,
            max_decoding_message_size: None,
            max_encoding_message_size: None,
        }
    }

    #[must_use]
    pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
        self.max_decoding_message_size = Some(limit);
        self
    }

    #[must_use]
    pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
        self.max_encoding_message_size = Some(limit);
        self
    }
}

impl<T> Clone for FlightServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_decoding_message_size: self.max_decoding_message_size,
            max_encoding_message_size: self.max_encoding_message_size,
        }
    }
}

impl<T> tonic::server::NamedService for FlightServiceServer<T> {
    const NAME: &'static str = SERVICE_NAME;
}

impl<T, B> Service<http::Request<B>> for FlightServiceServer<T>
where
    T: FlightService,
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let inner = self.inner.clone();
        let (dec, enc) = (
            self.max_decoding_message_size,
            self.max_encoding_message_size,
        );
        match req.uri().path() {
            HANDSHAKE => Box::pin(async move {
                let svc = StreamingFn(move |r| {
                    let inner = inner.clone();
                    async move { inner.handshake(r).await }
                });
                let mut grpc = tonic::server::Grpc::new(PbCodec::<
                    HandshakeResponse,
                    HandshakeRequest,
                >::default())
                .apply_max_message_size_config(dec, enc);
                Ok(grpc.streaming(svc, req).await)
            }),
            GET_FLIGHT_INFO => Box::pin(async move {
                let svc = UnaryFn(move |r| {
                    let inner = inner.clone();
                    async move { inner.get_flight_info(r).await }
                });
                let mut grpc =
                    tonic::server::Grpc::new(PbCodec::<FlightInfo, FlightDescriptor>::default())
                        .apply_max_message_size_config(dec, enc);
                Ok(grpc.unary(svc, req).await)
            }),
            GET_SCHEMA => Box::pin(async move {
                let svc = UnaryFn(move |r| {
                    let inner = inner.clone();
                    async move { inner.get_schema(r).await }
                });
                let mut grpc =
                    tonic::server::Grpc::new(PbCodec::<SchemaResult, FlightDescriptor>::default())
                        .apply_max_message_size_config(dec, enc);
                Ok(grpc.unary(svc, req).await)
            }),
            DO_GET => Box::pin(async move {
                let svc = ServerStreamingFn(move |r| {
                    let inner = inner.clone();
                    async move { inner.do_get(r).await }
                });
                let mut grpc = tonic::server::Grpc::new(PbCodec::<FlightData, Ticket>::default())
                    .apply_max_message_size_config(dec, enc);
                Ok(grpc.server_streaming(svc, req).await)
            }),
            DO_PUT => Box::pin(async move {
                let svc = StreamingFn(move |r| {
                    let inner = inner.clone();
                    async move { inner.do_put(r).await }
                });
                let mut grpc =
                    tonic::server::Grpc::new(PbCodec::<PutResult, FlightData>::default())
                        .apply_max_message_size_config(dec, enc);
                Ok(grpc.streaming(svc, req).await)
            }),
            _ => Box::pin(async move {
                let mut response = http::Response::new(tonic::body::Body::default());
                let headers = response.headers_mut();
                headers.insert(
                    tonic::Status::GRPC_STATUS,
                    (tonic::Code::Unimplemented as i32).into(),
                );
                headers.insert(
                    http::header::CONTENT_TYPE,
                    tonic::metadata::GRPC_CONTENT_TYPE,
                );
                Ok(response)
            }),
        }
    }
}

// ---- client ---------------------------------------------------------------

/// A small Flight client over a `tonic` channel, for tests and tooling.
#[derive(Clone)]
pub struct FlightClient {
    inner: tonic::client::Grpc<tonic::transport::Channel>,
}

impl FlightClient {
    pub fn new(channel: tonic::transport::Channel) -> Self {
        Self {
            inner: tonic::client::Grpc::new(channel),
        }
    }

    async fn ready(&mut self) -> Result<(), Status> {
        self.inner
            .ready()
            .await
            .map_err(|e| Status::unknown(format!("service was not ready: {e}")))
    }

    pub async fn get_flight_info(
        &mut self,
        request: impl tonic::IntoRequest<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        self.ready().await?;
        let path = http::uri::PathAndQuery::from_static(GET_FLIGHT_INFO);
        self.inner
            .unary(
                request.into_request(),
                path,
                PbCodec::<FlightDescriptor, FlightInfo>::default(),
            )
            .await
    }

    pub async fn do_get(
        &mut self,
        request: impl tonic::IntoRequest<Ticket>,
    ) -> Result<Response<Streaming<FlightData>>, Status> {
        self.ready().await?;
        let path = http::uri::PathAndQuery::from_static(DO_GET);
        self.inner
            .server_streaming(
                request.into_request(),
                path,
                PbCodec::<Ticket, FlightData>::default(),
            )
            .await
    }

    pub async fn do_put(
        &mut self,
        request: impl tonic::IntoStreamingRequest<Message = FlightData>,
    ) -> Result<Response<Streaming<PutResult>>, Status> {
        self.ready().await?;
        let path = http::uri::PathAndQuery::from_static(DO_PUT);
        self.inner
            .streaming(
                request.into_streaming_request(),
                path,
                PbCodec::<FlightData, PutResult>::default(),
            )
            .await
    }
}
