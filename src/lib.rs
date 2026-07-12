use std::net::TcpListener;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, web};
use actix_web::dev::Server;

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}

//async fn new_subscriber(req: HttpRequest) -> impl Responder {
    //todo!("Implement subscriptions.")
//}

pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(health_check))
            //.route("/subscriptions", web::post().to(new_subscriber))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
