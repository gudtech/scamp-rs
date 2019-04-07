extern crate scamp;
use scamp::agent::Agent;
use scamp::Message;

#[test]
fn it_starts () {

    let agent = Agent::new();

    fn greet(message: &Message)  {
        //let to = req.match_info().get("name").unwrap_or("World");
        println!("Hello World!");
    };

    agent.registerAction( "test.hello_world", greet );


}

