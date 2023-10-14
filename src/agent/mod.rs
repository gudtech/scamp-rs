use crate::error::Error;
use crate::message::Message;
use crate::transport::Transport;
use crate::Action;
use std::collections::HashMap;

use tokio::runtime::Runtime;

pub struct Agent {
    transports: Vec<Transport>,
    runtime: Option<Runtime>,
    actions: HashMap<String, Action>, //discovery:
}

impl Agent {
    pub fn new() -> Result<Self, Error> {
        let me = Agent {
            transports: Vec::new(),
            runtime: None,
            actions: HashMap::new(),
        };

        Ok(me)
        //        if (argp.opt('pidfile'))
        //        fs.writeFileSync(argp.opt('pidfile'), process.pid);
        //
        //        if (params.handleSignals !== false) {
        //            soa_util.update_service_file(params.tag, true);
        //        }
        //
        //        me.actions = params.actions || [];
        //        me.ident = params.tag + '-' + crypto.randomBytes( 18 ).toString('base64');
        //
        //        var key = me.key = fs.readFileSync(soa.config().val(params.tag + '.soa_key', '/etc/GT_private/services/' + params.tag + '.key'));
        //        var crt = me.cert = fs.readFileSync(soa.config().val(params.tag + '.soa_cert', '/etc/GT_private/services/' + params.tag + '.crt'));
        //
        //        me._classes = {};
        //        me._actions = {};
        //
        //        me.announcer = soa.module('discovery/announce').create({
        //            ident: me.ident,
        //            key: key,
        //            cert: crt,
        //            sector: params.sector || 'main',
        //            envelopeTypes: params.envelopes || ['json'],
        //        });
        //        me.announcer.setClasses( me._classes );
        //
        //        me._activeRequests = 0;
        //        me.listener  = soa.listener('beepish').create({
        //            callback: me.handleRequest.bind(me),
        //            key: key,
        //            cert: crt,
        //            listen: function (iface, uri) {
        //                me.announcer.addAddress(iface, uri);
        //            }
        //        });
        //        me.announcer.on('made_packet', (pkt) => me.listener.putSubdata('announce', '', pkt));
        //
        //        if (params.handleSignals !== false) {
        //            var stop_and_exit = () => {
        //                console.log('preparing to stop...');
        //                me.stopService().then(() => {
        //                    soa_util.update_service_file(params.tag, false);
        //                    process.exit(1);
        //                });
        //            };
        //            process.on('SIGINT', stop_and_exit);
        //            process.on('SIGTERM', stop_and_exit);
        //        }
        //
        //        soa.logger().configure({
        //            tag: params.tag,
        //            logsink_topic: params.logsink_topic,
        //            debug: argp.opt('debug')
        //        });
        //
        //        me.registerAction(
        //        `${params.tag}.queue_depth`,
        //        me.cookedHandler(() => { return { queue_depth: me._activeRequests - 1 } })
        //        );
    }
    pub fn register_action(&mut self, action: Action) -> Result<(), Error> {
        println!("registerAction {}", action.name);

        //        self.bindTransport()?;

        //TODO - do something nicer if the same action is registered twice
        self.actions.insert(action.name.to_owned(), action);

        Ok(())
    }

    pub fn find_action(&mut self, name: &str) -> Result<&Action, Error> {
        if let Some(action) = self.actions.get(name) {
            Ok(action)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Action Not Found").into())
        }
    }
    //    pub async fn call_action_async(&mut self, name: &str, message: Message) -> Result<Message,Error> {
    //        Ok(Message::empty())
    //    }

    //    fn bindTransport (&mut self) -> Result<(), Error> {
    //
    //        //TODO: make it search for the right kind of transport
    //        if self.transports.len() > 0{
    //            return Ok(());
    //        }
    //
    //        let transport = Transport::new( self )?;
    //        self.transports.push(transport);
    //
    //        Ok(())
    //    }

    pub fn tokio_runtime(&mut self) -> Result<&mut Runtime, Error> {
        match self.runtime {
            Some(ref mut rt) => Ok(rt),
            None => {
                let rt = Runtime::new()?;
                self.runtime = Some(rt);
                Ok(self.runtime.as_mut().unwrap())
            }
        }
    }
}
