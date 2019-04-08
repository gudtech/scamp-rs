use std::net::{SocketAddr,IpAddr};
use tokio::{net::TcpListener};
use futures::stream::Stream;
use futures::future::Future;
use crate::Error;

use crate::agent::Agent;

pub struct Beepish{
    // TODO - add shutdown signal here
}

impl Beepish {
    pub fn new (agent: &mut Agent) -> Result<Self, Error>{

        let listener: TcpListener = bind_listener()?;


        let server = listener.incoming()
            .for_each(|socket| {

                //TODO: better understand if the default toko executor is acceptable, and why
                //tokio::spawn();

                //TODO: Process socket
                Ok(())
            }).map_err(|err| {
                // Handle error by printing to STDOUT.
                println!("accept error = {:?}", err);
            });


        agent.tokio_runtime()?.spawn(server);

        let transport = Beepish{

        };

        Ok(transport)
    }
}

fn bind_listener() -> Result<TcpListener, Error>{

    let tries : u16 = 20;   //TODO: get config beepish.bind_tries
    let pmin : u16 = 30100; //TODO: get config beepish.first_port
    let pmax : u16 = 30399; //TODO: get config beepish.last_port

    let mut rng = rand::thread_rng();

    //TODO: perform host name resolution or interface specific address search
    //TODO: Binding to multiple interfaces

    let ip: IpAddr = "127.0.0.1".parse().unwrap();

    for _ in 0..tries {
        let port : u16 = 4200; //rng.gen_range(pmin, pmax + 1);
        let addr = SocketAddr::new(ip, port);

        let listener : TcpListener;

        match TcpListener::bind(&addr) {
            Ok(l) => {
                return Ok(l);
            }
            Err(e) => {
                println!("Bind failed {} - trying again", e);
            }
        };
    };

    return Err(std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, format!("Failed to bind to address {}", ip)).into())
}