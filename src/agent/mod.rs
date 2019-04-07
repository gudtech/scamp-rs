use crate::message::Message;

pub struct Agent {
    discovery: Option<>
}

impl Agent {
    pub fn new (){

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
//        me.server  = soa.server('beepish').create({
//            callback: me.handleRequest.bind(me),
//            key: key,
//            cert: crt,
//            listen: function (iface, uri) {
//                me.announcer.addAddress(iface, uri);
//            }
//        });
//        me.announcer.on('made_packet', (pkt) => me.server.putSubdata('announce', '', pkt));
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
}