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

#![forbid(unsafe_code)]
#![warn(
    anonymous_parameters,
    missing_copy_implementations,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unused_extern_crates,
    unused_qualifications,
    variant_size_differences
)]
mod config;
pub mod dao;
mod error;
mod password;
pub mod proxy;

use crate::config::Config;
use crate::dao::Dao;
use crate::proxy::handler::proxy_handler;
use crate::proxy::upstream::Upstreams;
use actix_files::Files;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use log::info;
use std::env;
use std::time::Duration;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(config_path) = args.get(1) {
        let config = Config::read(config_path).await.unwrap();
        Dao::init(config.app_run_directory()).await.unwrap();
        start_http().await;
    } else {
        let config = Config::new().unwrap();
        let path = "vaden.yaml";
        config.save(path).await.unwrap();
        println!("Config file saved in {}", path);
    }
}

async fn start_http() {
    let upstreams1 = Data::new(Upstreams::default());
    let upstreams2 = upstreams1.clone();
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let panel_handle = tokio::spawn(
        HttpServer::new(move || {
            App::new()
                .app_data(upstreams1.clone())
                .service(Files::new("/static", "../vaden-frontend/dist").index_file("index.html"))
        })
        .bind(("0.0.0.0", 8081))
        .unwrap()
        .run(),
    );

    let proxy_handle = tokio::spawn(
        HttpServer::new(move || {
            App::new()
                .app_data(upstreams2.clone())
                .default_service(web::to(proxy_handler))
        })
        .bind(("0.0.0.0", 8080))
        .unwrap()
        .run(),
    );
    let mut signal = signal(SignalKind::interrupt()).unwrap();
    select! {
        _ = signal.recv() => {
            info!("received SIGINT, exiting...");
            tokio::time::sleep(Duration::from_millis(100)).await;
            std::process::exit(0);
        },
        _ = proxy_handle => {
            println!("Proxy exited");
        },
        _ = panel_handle => {
            println!("Panel exited");
        }
    }
}
