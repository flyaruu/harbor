use anyhow::{Context, Result};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::config::AppConfig;
use crate::{convert_tile_bytes_to_glb, tile_io};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileRoute {
    zoom: u8,
    x: u32,
    y: u32,
}

pub(crate) fn run(config: &AppConfig) -> Result<()> {
    let address = format!("{}:{}", config.server.bind, config.server.port);
    let server = Server::http(&address)
        .map_err(|error| anyhow::anyhow!("failed to bind server on {address}: {error}"))?;

    println!(
        "Serving GLB tiles on http://{}:{} via {}",
        config.server.bind, config.server.port, config.server.backend
    );

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(request, config) {
            eprintln!("server request error: {error:#}");
        }
    }

    Ok(())
}

fn handle_request(request: Request, config: &AppConfig) -> Result<()> {
    if request.method() != &Method::Get {
        return respond_plain(request, StatusCode(405), "method not allowed");
    }

    let Some(route) = parse_tile_route(request.url()) else {
        return respond_plain(request, StatusCode(404), "not found");
    };

    let backend_url = build_backend_url(&config.server.backend, route);
    let bytes = match tile_io::fetch_tile_bytes(&backend_url) {
        Ok(bytes) => bytes,
        Err(error) => {
            return respond_plain(
                request,
                StatusCode(502),
                &format!("failed to fetch backend tile: {error:#}"),
            );
        }
    };

    let glb = match convert_tile_bytes_to_glb(&bytes, &config.conversion) {
        Ok(glb) => glb,
        Err(error) => {
            return respond_plain(
                request,
                StatusCode(500),
                &format!("failed to convert tile: {error:#}"),
            );
        }
    };

    let response = Response::from_data(glb).with_header(content_type_header()?);
    request
        .respond(response)
        .context("failed to write HTTP response")
}

fn respond_plain(request: Request, status: StatusCode, body: &str) -> Result<()> {
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(text_content_type_header()?);
    request
        .respond(response)
        .context("failed to write HTTP response")
}

fn content_type_header() -> Result<Header> {
    Header::from_bytes(&b"Content-Type"[..], &b"model/gltf-binary"[..])
        .map_err(|_| anyhow::anyhow!("failed to build content type header"))
}

fn text_content_type_header() -> Result<Header> {
    Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
        .map_err(|_| anyhow::anyhow!("failed to build text content type header"))
}

fn parse_tile_route(url: &str) -> Option<TileRoute> {
    let path = url.split('?').next()?;
    let segments = path.split('/').collect::<Vec<_>>();
    let ["", "data", zoom, x, y] = segments.as_slice() else {
        return None;
    };
    let y = y.strip_suffix(".glb")?;

    Some(TileRoute {
        zoom: zoom.parse().ok()?,
        x: x.parse().ok()?,
        y: y.parse().ok()?,
    })
}

fn build_backend_url(backend: &str, route: TileRoute) -> String {
    let base = backend.trim_end_matches('/');
    format!("{base}/data/v3/{}/{}/{}.pbf", route.zoom, route.x, route.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tile_route() {
        assert_eq!(
            parse_tile_route("/data/14/8396/5421.glb"),
            Some(TileRoute {
                zoom: 14,
                x: 8396,
                y: 5421,
            })
        );
    }

    #[test]
    fn parses_tile_route_with_query_string() {
        assert_eq!(
            parse_tile_route("/data/14/8396/5421.glb?cache=0"),
            Some(TileRoute {
                zoom: 14,
                x: 8396,
                y: 5421,
            })
        );
    }

    #[test]
    fn rejects_invalid_tile_route() {
        assert_eq!(parse_tile_route("/data/14/8396/5421.pbf"), None);
        assert_eq!(parse_tile_route("/tiles/14/8396/5421.glb"), None);
        assert_eq!(parse_tile_route("/data/14/8396/not-a-number.glb"), None);
    }

    #[test]
    fn builds_backend_url() {
        assert_eq!(
            build_backend_url(
                "http://localhost:8080",
                TileRoute {
                    zoom: 14,
                    x: 8396,
                    y: 5421,
                }
            ),
            "http://localhost:8080/data/v3/14/8396/5421.pbf"
        );
    }
}
