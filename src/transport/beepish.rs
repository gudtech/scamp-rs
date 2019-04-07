use std::net::{SocketAddr,IpAddr};
use tokio::{runtime::Runtime,net::TcpListener};
use futures::stream::Stream;
use futures::future::Future;

use crate::agent::Agent;

pub struct Beepish{
    listener: TcpListener,
    runtime: Runtime,
}

impl Beepish {
    pub fn new (agent: &mut Agent) -> Result<Self, impl std::error::Error>{

        let tries : u16 = 20;   //TODO: get config beepish.bind_tries
        let pmin : u16 = 30100; //TODO: get config beepish.first_port
        let pmax : u16 = 30399; //TODO: get config beepish.last_port

        let mut rng = rand::thread_rng();

        //TODO: perform host name resolution or interface specific address search
        //TODO: Binding to multiple interfaces

        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        let listener: TcpListener;

        for _ in 0..tries {
            let port : u16 = 4200; //rng.gen_range(pmin, pmax + 1);
            let addr = SocketAddr::new(ip, port);

            let listener : TcpListener;

            match TcpListener::bind(&addr) {
                Ok(l) => {
                    listener = l;
                    break;
                }
                Err(e) => {
                    println!("Bind failed {} - trying again", e);
                }
            };
        };

        let server = listener.incoming()
            .for_each(|socket| {
                //TODO: Process socket
                Ok(())
            }).map_err(|err| {
                // Handle error by printing to STDOUT.
                println!("accept error = {:?}", err);
            });


        let runtime = agent.tokio_runtime();
        runtime.spawn(server);

        let transport = Beepish{
            listener,
            runtime
        };

        Ok(transport)
    }
}