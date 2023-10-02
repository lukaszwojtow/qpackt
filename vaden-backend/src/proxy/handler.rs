// SPDX-License-Identifier: AGPL-3.0
/*
   Vaden: Versioned Application Deployment Engine
   Copyright (C) 2023 Łukasz Wojtów

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU Affero General Public License as
   published by the Free Software Foundation, either version 3 of the
   License, or (at your option) any later version.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU Affero General Public License for more details.

   You should have received a copy of the GNU Affero General Public License
   along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use crate::proxy::upstream::Upstream;
use actix_web::web::{Data, Payload};
use actix_web::{HttpRequest, HttpResponse};
use awc::Client;
use tokio::sync::RwLock;

pub(crate) async fn proxy_handler(
    payload: Payload,
    client_request: HttpRequest,
    upstreams: Data<RwLock<Vec<Upstream>>>,
) -> HttpResponse {
    let mut destination = upstreams.read().await[0].url().clone();
    destination.set_path(client_request.uri().path());
    destination.set_query(client_request.uri().query());

    let client = Client::default();
    let proxy_request = client
        .request_from(destination.as_str(), client_request.head())
        .no_decompress();
    let upstream_response = proxy_request.send_stream(payload).await.unwrap();
    let mut proxy_response = HttpResponse::build(upstream_response.status());
    for (header_name, header_value) in upstream_response
        .headers()
        .iter()
        .filter(|(h, _)| *h != "connection")
    {
        proxy_response.insert_header((header_name.clone(), header_value.clone()));
    }
    proxy_response.streaming(upstream_response)
}
