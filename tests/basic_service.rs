// extern crate scamp;
// use scamp::agent::Agent;
// use scamp::Message;
// use scamp::Error;
// use scamp::action::rpc_action;

// #[test]
// fn it_starts () -> Result<(),Error>{

//     let mut agent = Agent::new()?;

//     fn greet(message: Message) -> Result<Message,Error> {
//         //let to = req.match_info().get("name").unwrap_or("World");
//         println!("Hello World!");
//         Ok(Message::empty())
//     };

//     agent.register_action( rpc_action("test.hello_world", greet) )?;

//     let action = agent.find_action("test.hello_world")?;
//     action.call_blocking(Message::empty())?;

//     Ok(())
// }
