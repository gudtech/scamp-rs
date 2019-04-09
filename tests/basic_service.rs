extern crate scamp;
use scamp::agent::Agent;
use scamp::Message;
use scamp::Error;
use scamp::action::rpc_action;

#[test]
fn it_starts () -> Result<(),Error>{

    let mut agent = Agent::new()?;

    async fn greet(message: Message) -> Message {
        //let to = req.match_info().get("name").unwrap_or("World");
        println!("Hello World!");
        Ok(Message::empty())
    };

    agent.registerAction( rpc_action("test.hello_world", greet) )?;


    agent.callAction("test.hello_world", Message::empty());

    Ok(())
}

