// SPDX-License-Identifier: AGPL-3.0
/*
   Vaden: Versioned Application Deployment Engine
   Copyright (C) 2023 Łukasz Wojtów

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU Affero General Public License as
   published by the Free Software Foundation, either version 3 of the
   License.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU Affero General Public License for more details.

   You should have received a copy of the GNU Affero General Public License
   along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use crate::config::Config;
use crate::dao::Dao;
use crate::panel::versions::upload::upload_version;
use crate::proxy::upstream::Upstreams;
use actix_files::Files;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use tokio::task::JoinHandle;

pub(super) mod versions;

pub(super) fn start_panel_http(
    upstreams: Data<Upstreams>,
    config: Data<Config>,
    dao: Data<Dao>,
) -> JoinHandle<std::io::Result<()>> {
    tokio::spawn({
        let app_config = config.clone();
        HttpServer::new(move || {
            App::new()
                .app_data(upstreams.clone())
                .app_data(app_config.clone())
                .app_data(dao.clone())
                .service(web::resource("/upload-version").route(web::post().to(upload_version)))
                // This needs to be at the end of all `service` calls so that backend (api) calls will have a chance to match routes.
                .service(Files::new("/", "../vaden-frontend/dist").index_file("index.html"))
        })
        .bind(config.panel_addr())
        .unwrap()
        .run()
    })
}
