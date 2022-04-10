// Copyright (c) 2022, 37 Miners, LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[allow(deprecated)]
use clap::load_yaml;
use clap::Command;
use nioruntime_deps::dirs;
use nioruntime_deps::fsutils;
use nioruntime_deps::path_clean::clean as path_clean;
use nioruntime_err::{Error, ErrorKind};
use nioruntime_evh::TLSServerConfig;
use nioruntime_evh::{ConnectionData, EventHandlerConfig};
use nioruntime_http::{send_websocket_message, ApiContext, HttpConfig, HttpServer, ListenerType};
use nioruntime_log::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

info!();

const POST_BYTES: &[u8] = "/post".as_bytes();
const EMPTY_REPLY: &[u8] = b"HTTP/1.1 200 Ok\r\n\
Server: nioruntime httpd/0.0.3-beta.1\r\n\
Content-Type: text/html\r\n\
Content-Length: 8\r\n\
Connection: close\r\n\r\n\
Empty.\r\n";

// include build information
pub mod built_info {
	include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

fn main() {
	match real_main() {
		Ok(_) => {}
		Err(e) => {
			println!("Startup error: {} Halting!", e.inner());
		}
	}
}

fn real_main() -> Result<(), Error> {
	let start = Instant::now();
	#[allow(deprecated)]
	let yml = load_yaml!("nio.yml");
	#[allow(deprecated)]
	let cmd = Command::from_yaml(yml).version(built_info::PKG_VERSION);
	let args = cmd.clone().get_matches();

	let file_args = match args.is_present("config") {
		true => {
			let mut lines = vec![];
			lines.push("fileargs".to_string());
			let file = File::open(args.value_of("config").unwrap())?;
			for line in BufReader::new(file).lines() {
				let line = line?;
				let line = line.trim();
				if line.find("#") == Some(0) {
					continue; // comments
				}
				for line in line.split_ascii_whitespace() {
					lines.push(line.to_string());
				}
			}
			cmd.get_matches_from(lines)
		}
		false => {
			let lines: Vec<String> = vec![];
			#[allow(deprecated)]
			Command::from_yaml(yml)
				.version(built_info::PKG_VERSION)
				.get_matches_from(lines)
		}
	};

	let mut listeners = match args.is_present("listener") {
		true => {
			let mut listeners = vec![];
			let mut listener_args = vec![];

			for listener in args.values_of("listener").unwrap() {
				listener_args.push(listener);
			}

			let mut i = 0;
			loop {
				if i >= listener_args.len() {
					break;
				}
				let listener = listener_args[i];
				let mut spl = listener.split(':');
				let proto = match spl.next() {
					Some(proto) => proto,
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let sock_addr = match spl.next() {
					Some(sock_addr) => &sock_addr[2..],
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let sock_addr = match spl.next() {
					Some(port) => format!("{}:{}", sock_addr, port),
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let tls_config = if proto == "https" {
					i += 1;
					if i >= listener_args.len() {
						return Err(ErrorKind::Configuration(
							"malformed listener. private_key_file required for https".into(),
						)
						.into());
					}
					let private_key_file = listener_args[i].to_string();
					println!("private_key_file={}", private_key_file);

					i += 1;
					if i >= listener_args.len() {
						return Err(ErrorKind::Configuration(
							"malformed listener. certificates_file required for https".into(),
						)
						.into());
					}

					let certificates_file = listener_args[i].to_string();

					i += 1;
					if i >= listener_args.len() {
						return Err(ErrorKind::Configuration(
							"malformed listener. sni_host required for https".into(),
						)
						.into());
					}

					let sni_host = listener_args[i].to_string();

					Some(TLSServerConfig {
						private_key_file,
						certificates_file,
						sni_host,
					})
				} else {
					None
				};

				let listener_type = if proto == "http" {
					ListenerType::Plain
				} else if proto == "https" {
					ListenerType::Tls
				} else {
					return Err(ErrorKind::Configuration(
						"malformed listener (http or https only)".into(),
					)
					.into());
				};
				listeners.push((listener_type, SocketAddr::from_str(&sock_addr)?, tls_config));
				i += 1;
			}
			listeners
		}
		false => vec![],
	};

	match file_args.is_present("listener") {
		true => {
			for listener in file_args.values_of("listener").unwrap() {
				let mut spl = listener.split(':');
				let proto = match spl.next() {
					Some(proto) => proto,
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let sock_addr = match spl.next() {
					Some(sock_addr) => &sock_addr[2..],
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let sock_addr = match spl.next() {
					Some(port) => format!("{}:{}", sock_addr, port),
					None => {
						return Err(ErrorKind::Configuration("malformed listener".into()).into());
					}
				};

				let tls_config = if proto == "https" {
					let private_key_file = match spl.next() {
						Some(private_key_file) => private_key_file,
						None => {
							return Err(ErrorKind::Configuration(
								"malformed listener. private_key_file required for https".into(),
							)
							.into());
						}
					}
					.to_string();

					let certificates_file = match spl.next() {
						Some(certificates_file) => certificates_file,
						None => {
							return Err(ErrorKind::Configuration(
								"malformed listener. private_key_file required for https".into(),
							)
							.into());
						}
					}
					.to_string();

					let sni_host = match spl.next() {
						Some(sni_host) => sni_host.to_string(),
						None => {
							return Err(ErrorKind::Configuration(
								"malformed listener. sni_host required for https".into(),
							)
							.into());
						}
					}
					.to_string();

					Some(TLSServerConfig {
						private_key_file,
						certificates_file,
						sni_host,
					})
				} else {
					None
				};

				let listener_type = if proto == "http" {
					ListenerType::Plain
				} else if proto == "https" {
					ListenerType::Tls
				} else {
					return Err(ErrorKind::Configuration(
						"malformed listener (http or https only)".into(),
					)
					.into());
				};
				listeners.push((listener_type, SocketAddr::from_str(&sock_addr)?, tls_config));
			}
		}
		false => {}
	}

	if listeners.len() == 0 {
		listeners.push((
			ListenerType::Plain,
			SocketAddr::from_str("127.0.0.1:8080")?,
			None,
		));
	}

	let (mut virtual_ips, mut virtual_hosts) = match args.is_present("virtual_server") {
		true => {
			let mut virtual_servers = vec![];
			for virtual_server in args.values_of("virtual_server").unwrap() {
				virtual_servers.push(virtual_server);
			}

			if virtual_servers.len() % 2 != 0 {
				return Err(ErrorKind::Configuration(
                                    "malformed --virtual_servers. Format is: --virtual_servers <host/ip> <directory>".into()
                            ).into());
			}

			let mut i = 0;
			let mut virtual_ips = HashMap::new();
			let mut virtual_hosts = HashMap::new();
			loop {
				if i >= virtual_servers.len() {
					break;
				}

				println!("parsing {}", virtual_servers[i]);
				match SocketAddr::from_str(virtual_servers[i]) {
					Ok(ip_addr) => {
						virtual_ips.insert(ip_addr, virtual_servers[i + 1].as_bytes().to_vec())
					}
					Err(e) => {
						println!("virtual_hostserr={}", e);
						virtual_hosts.insert(
							virtual_servers[i].as_bytes().to_vec(),
							virtual_servers[i + 1].as_bytes().to_vec(),
						)
					}
				};
				i += 2;
			}
			(virtual_ips, virtual_hosts)
		}
		false => (HashMap::new(), HashMap::new()),
	};

	match file_args.is_present("virtual_server") {
		true => {
			let mut virtual_servers = vec![];
			for virtual_server in file_args.values_of("virtual_server").unwrap() {
				virtual_servers.push(virtual_server);
			}

			let mut i = 0;
			loop {
				if i >= virtual_servers.len() {
					break;
				}
				match SocketAddr::from_str(virtual_servers[i]) {
					Ok(ip_addr) => {
						virtual_ips.insert(ip_addr, virtual_servers[i + 1].as_bytes().to_vec())
					}
					Err(_) => virtual_hosts.insert(
						virtual_servers[i].as_bytes().to_vec(),
						virtual_servers[i + 1].as_bytes().to_vec(),
					),
				};
				i += 2;
			}
		}
		false => {}
	}

	let threads = match args.is_present("threads") {
		true => args.value_of("threads").unwrap().parse()?,
		false => match file_args.is_present("threads") {
			true => file_args.value_of("threads").unwrap().parse()?,
			false => 8,
		},
	};

	let gzip_compression_level = match args.is_present("gzip_compression_level") {
		true => args.value_of("gzip_compression_level").unwrap().parse()?,
		false => match file_args.is_present("gzip_compression_level") {
			true => file_args
				.value_of("gzip_compression_level")
				.unwrap()
				.parse()?,
			false => 7,
		},
	};

	let gzip_extensions = match args.is_present("gzip_extensions") {
		true => {
			let mut gzip_extensions = HashSet::new();
			for gzip_extension in args.values_of("gzip_extensions").unwrap() {
				gzip_extensions.insert(gzip_extension.as_bytes().to_vec());
			}
			gzip_extensions
		}
		false => match file_args.is_present("gzip_extensions") {
			true => {
				let mut gzip_extensions = HashSet::new();
				for gzip_extension in file_args.values_of("gzip_extensions").unwrap() {
					gzip_extensions.insert(gzip_extension.as_bytes().to_vec());
				}
				gzip_extensions
			}
			false => HashSet::new(),
		},
	};

	let listen_queue_size = match args.is_present("listen_queue_size") {
		true => args.value_of("listen_queue_size").unwrap().parse()?,
		false => match file_args.is_present("listen_queue_size") {
			true => file_args.value_of("listen_queue_size").unwrap().parse()?,
			false => 1_000,
		},
	};

	let max_header_size = match args.is_present("max_header_size") {
		true => args.value_of("max_header_size").unwrap().parse()?,
		false => match file_args.is_present("max_header_size") {
			true => file_args.value_of("max_header_size").unwrap().parse()?,
			false => 16_384,
		},
	};

	let max_header_name_len = match args.is_present("max_header_name_len") {
		true => args.value_of("max_header_name_len").unwrap().parse()?,
		false => match file_args.is_present("max_header_name_len") {
			true => file_args.value_of("max_header_name_len").unwrap().parse()?,
			false => 128,
		},
	};

	let max_header_value_len = match args.is_present("max_header_value_len") {
		true => args.value_of("max_header_value_len").unwrap().parse()?,
		false => match file_args.is_present("max_header_value_len") {
			true => file_args
				.value_of("max_header_value_len")
				.unwrap()
				.parse()?,
			false => 1_024,
		},
	};

	let temp_dir = match args.is_present("temp_dir") {
		true => args.value_of("temp_dir").unwrap(),
		false => match file_args.is_present("temp_dir") {
			true => file_args.value_of("temp_dir").unwrap(),
			false => "~/.niohttpd/tmp",
		},
	}
	.to_string();

	let webroot = match args.is_present("webroot") {
		true => args.value_of("webroot").unwrap(),
		false => match file_args.is_present("webroot") {
			true => file_args.value_of("webroot").unwrap(),
			false => "~/.niohttpd/www",
		},
	};

	let mainlog = match args.is_present("mainlog") {
		true => args.value_of("mainlog").unwrap(),
		false => match file_args.is_present("mainlog") {
			true => file_args.value_of("mainlog").unwrap(),
			false => "~/.niohttpd/logs/mainlog.log",
		},
	}
	.to_string();

	let mainlog_max_age = match args.is_present("mainlog_max_age") {
		true => args.value_of("mainlog_max_age").unwrap().parse()?,
		false => match file_args.is_present("mainlog_max_age") {
			true => file_args.value_of("mainlog_max_age").unwrap().parse()?,
			false => 3_600_000,
		},
	};

	let mainlog_max_size = match args.is_present("mainlog_max_size") {
		true => args.value_of("mainlog_max_size").unwrap().parse()?,
		false => match file_args.is_present("mainlog_max_size") {
			true => file_args.value_of("mainlog_max_size").unwrap().parse()?,
			false => 1_000_000,
		},
	};

	let max_header_entries = match args.is_present("max_header_entries") {
		true => args.value_of("max_header_entries").unwrap().parse()?,
		false => match file_args.is_present("max_header_entries") {
			true => file_args.value_of("max_header_entries").unwrap().parse()?,
			false => 1_000,
		},
	};

	let max_cache_files = match args.is_present("max_cache_files") {
		true => args.value_of("max_cache_files").unwrap().parse()?,
		false => match file_args.is_present("max_cache_files") {
			true => file_args.value_of("max_cache_files").unwrap().parse()?,
			false => 1_000,
		},
	};

	let max_cache_chunks = match args.is_present("max_cache_chunks") {
		true => args.value_of("max_cache_chunks").unwrap().parse()?,
		false => match file_args.is_present("max_cache_chunks") {
			true => file_args.value_of("max_cache_chunks").unwrap().parse()?,
			false => 100,
		},
	};

	let cache_chunk_size = match args.is_present("cache_chunk_size") {
		true => args.value_of("cache_chunk_size").unwrap().parse()?,
		false => match file_args.is_present("cache_chunk_size") {
			true => file_args.value_of("cache_chunk_size").unwrap().parse()?,
			false => 1_048_576,
		},
	};

	let max_load_factor = match args.is_present("max_load_factor") {
		true => args.value_of("max_load_factor").unwrap().parse()?,
		false => match file_args.is_present("max_load_factor") {
			true => file_args.value_of("max_load_factor").unwrap().parse()?,
			false => 0.9,
		},
	};

	let max_bring_to_front = match args.is_present("max_bring_to_front") {
		true => args.value_of("max_bring_to_front").unwrap().parse()?,
		false => match file_args.is_present("max_bring_to_front") {
			true => file_args.value_of("max_bring_to_front").unwrap().parse()?,
			false => 1_000,
		},
	};

	let process_cache_update = match args.is_present("process_cache_update") {
		true => args.value_of("process_cache_update").unwrap().parse()?,
		false => match file_args.is_present("process_cache_update") {
			true => file_args
				.value_of("process_cache_update")
				.unwrap()
				.parse()?,
			false => 1_000,
		},
	};

	let cache_recheck_fs_millis = match args.is_present("cache_recheck_fs_millis") {
		true => args.value_of("cache_recheck_fs_millis").unwrap().parse()?,
		false => match file_args.is_present("cache_recheck_fs_millis") {
			true => file_args
				.value_of("cache_recheck_fs_millis")
				.unwrap()
				.parse()?,
			false => 3_000,
		},
	};

	let connect_timeout = match args.is_present("connect_timeout") {
		true => args.value_of("connect_timeout").unwrap().parse()?,
		false => match file_args.is_present("connect_timeout") {
			true => file_args.value_of("connect_timeout").unwrap().parse()?,
			false => 30_000,
		},
	};

	let idle_timeout = match args.is_present("idle_timeout") {
		true => args.value_of("idle_timeout").unwrap().parse()?,
		false => match file_args.is_present("idle_timeout") {
			true => file_args.value_of("idle_timeout").unwrap().parse()?,
			false => 60_000,
		},
	};

	let read_buffer_size = match args.is_present("read_buffer_size") {
		true => args.value_of("read_buffer_size").unwrap().parse()?,
		false => match file_args.is_present("read_buffer_size") {
			true => file_args.value_of("read_buffer_size").unwrap().parse()?,
			false => 10_240,
		},
	};

	let max_rwhandles = match args.is_present("max_rwhandles") {
		true => args.value_of("max_rwhandles").unwrap().parse()?,
		false => match file_args.is_present("max_rwhandles") {
			true => file_args.value_of("max_rwhandles").unwrap().parse()?,
			false => 16_000,
		},
	};

	let max_active_connections = match args.is_present("max_active_connections") {
		true => args.value_of("max_active_connections").unwrap().parse()?,
		false => match file_args.is_present("max_active_connections") {
			true => file_args
				.value_of("max_active_connections")
				.unwrap()
				.parse()?,
			false => max_rwhandles,
		},
	};

	let max_async_connections = match args.is_present("max_async_connections") {
		true => args.value_of("max_async_connections").unwrap().parse()?,
		false => match file_args.is_present("max_async_connections") {
			true => file_args
				.value_of("max_async_connections")
				.unwrap()
				.parse()?,
			false => max_rwhandles,
		},
	};

	let max_handle_numeric_value = match args.is_present("max_handle_numeric_value") {
		true => args.value_of("max_handle_numeric_value").unwrap().parse()?,
		false => match file_args.is_present("max_handle_numeric_value") {
			true => file_args
				.value_of("max_handle_numeric_value")
				.unwrap()
				.parse()?,
			false => 16_100,
		},
	};

	let housekeeper_frequency = match args.is_present("housekeeper_frequency") {
		true => args.value_of("housekeeper_frequency").unwrap().parse()?,
		false => match file_args.is_present("housekeeper_frequency") {
			true => file_args
				.value_of("housekeeper_frequency")
				.unwrap()
				.parse()?,
			false => 1_000,
		},
	};

	let content_upload_slab_size = match args.is_present("content_upload_slab_size") {
		true => args.value_of("content_upload_slab_size").unwrap().parse()?,
		false => match file_args.is_present("content_upload_slab_size") {
			true => file_args
				.value_of("content_upload_slab_size")
				.unwrap()
				.parse()?,
			false => 1_024,
		},
	};

	let content_upload_slab_count = match args.is_present("content_upload_slab_count") {
		true => args
			.value_of("content_upload_slab_count")
			.unwrap()
			.parse()?,
		false => match file_args.is_present("content_upload_slab_count") {
			true => file_args
				.value_of("content_upload_slab_count")
				.unwrap()
				.parse()?,
			false => 1_024,
		},
	};

	let max_content_len = match args.is_present("max_content_len") {
		true => args.value_of("max_content_len").unwrap().parse()?,
		false => match file_args.is_present("max_content_len") {
			true => file_args.value_of("max_content_len").unwrap().parse()?,
			false => 1_048_576,
		},
	};

	let show_request_headers = match args.is_present("show_request_headers") {
		true => true,
		false => file_args.is_present("show_request_headers"),
	};

	let show_response_headers = match args.is_present("show_response_headers") {
		true => true,
		false => file_args.is_present("show_response_headers"),
	};

	let debug = match args.is_present("debug") {
		true => true,
		false => file_args.is_present("debug"),
	};

	let debug_api = match args.is_present("debug_api") {
		true => true,
		false => file_args.is_present("debug_api"),
	};

	let debug_websocket = match args.is_present("debug_websocket") {
		true => true,
		false => file_args.is_present("debug_websocket"),
	};

	let config = HttpConfig {
		start,
		content_upload_slab_count,
		content_upload_slab_size,
		max_content_len,
		temp_dir,
		listeners,
		show_request_headers,
		show_response_headers,
		listen_queue_size,
		max_header_size,
		max_header_entries,
		max_header_name_len,
		max_header_value_len,
		max_cache_files,
		max_cache_chunks,
		cache_chunk_size,
		max_load_factor,
		max_bring_to_front,
		process_cache_update,
		cache_recheck_fs_millis,
		gzip_compression_level,
		gzip_extensions,
		connect_timeout,
		idle_timeout,
		mainlog_max_age,
		mainlog_max_size,
		max_active_connections,
		max_async_connections,
		virtual_ips,
		virtual_hosts,
		webroot: webroot.as_bytes().to_vec(),
		debug,
		debug_api,
		debug_websocket,
		evh_config: EventHandlerConfig {
			threads,
			housekeeper_frequency,
			max_handle_numeric_value,
			max_rwhandles,
			read_buffer_size,
			..Default::default()
		},
		mainlog,
		..Default::default()
	};

	let home_dir = match dirs::home_dir() {
		Some(p) => p,
		None => PathBuf::new(),
	}
	.as_path()
	.display()
	.to_string();

	let mainlog = config.mainlog.replace("~", &home_dir);
	let mainlog = path_clean(&mainlog);

	let mut p = PathBuf::from(mainlog.clone());
	p.pop();
	fsutils::mkdir(&p.as_path().display().to_string());

	log_config!(LogConfig {
		show_line_num: false,
		show_log_level: false,
		show_bt: false,
		file_path: Some(mainlog),
		max_age_millis: config.mainlog_max_age,
		max_size: config.mainlog_max_size,
		auto_rotate: false,
		..Default::default()
	})?;

	let mut http = HttpServer::new(config)?;

	if debug_api {
		let mut mappings = std::collections::HashSet::new();
		mappings.insert("/post".as_bytes().to_vec());
		mappings.insert("/get".as_bytes().to_vec());
		http.set_api_config(nioruntime_http::HttpApiConfig {
			mappings,
			..Default::default()
		})?;
	}

	if debug_websocket {
		http.set_ws_handler(move |conn_data, message| {
			debug!(
				"conn[{}] received a websocket message: {:?}",
				conn_data.get_connection_id(),
				message
			)?;
			send_websocket_message(conn_data, &message)?;
			Ok(true)
		})?;
	}

	http.set_api_handler(move |conn_data, headers, ctx| {
		match headers.get_uri() {
			POST_BYTES => {
				ctx.set_async()?;

				let content_len = headers.content_len()?;
				let conn_data = conn_data.clone();
				let mut ctx = ctx.clone();
				std::thread::spawn(move || -> Result<(), Error> {
					match pull_post_thread(content_len, &mut ctx, &conn_data) {
						Ok(_) => {}
						Err(e) => {
							warn!("pull post generated error: {}", e)?;
						}
					}
					ctx.async_complete()?;
					Ok(())
				});
			}
			_ => {
				conn_data.write(EMPTY_REPLY)?;
			}
		}
		Ok(())
	})?;
	http.start()?;
	std::thread::park();
	Ok(())
}

fn pull_post_thread(
	content_len: usize,
	ctx: &mut ApiContext,
	conn_data: &ConnectionData,
) -> Result<(), Error> {
	let mut buf = vec![];
	buf.resize(content_len, 0u8);
	let len = ctx.pull_bytes(&mut buf)?;
	let display_str = if len > 10000 {
		format!("[{} bytes of data]", len)
	} else {
		match std::str::from_utf8(&buf[0..len]) {
			Ok(display_str) => display_str.to_string(),
			Err(_) => format!("[{} bytes of non-utf8data]", len),
		}
	};
	debug!("Debug post handler read {} bytes: {}", len, display_str,)?;
	let response = format!(
		"HTTP/1.1 200 Ok\r\nContent-Length: {}\r\n\r\nPost_Data was: {}",
		display_str.len() + 15,
		display_str
	);
	conn_data.write(response.as_bytes())?;
	Ok(())
}
