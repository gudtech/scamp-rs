extern crate scamp;
use scamp::agent::Agent;
use scamp::Message;
use scamp::Error;

#[test]
fn it_starts () -> Result<(),Error>{

    let mut agent = Agent::new()?;

    fn greet(message: Message) {
        //let to = req.match_info().get("name").unwrap_or("World");
        println!("Hello World!");
    };

    agent.registerAction( "test.hello_world", greet )?;


    agent.callAction("test.hello_world",hashmap!{ name: "Alice" });

    Ok(())
}

