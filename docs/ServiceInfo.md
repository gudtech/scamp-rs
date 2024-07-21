# ServiceInfo Packet Format

## Overview

`ServiceInfo` packets are used in SOA for service discovery and routing. The JSON body contains metadata about the service.

## JSON Structure

In Packet v3 or v4, the JSON structure is an array with 9 elements

### Common Fields

0. `packetVersion`: Always `3`
   Note: v4 packets also specify 3 in this field for some oddball historical reason
1. `identity`: Unique string identifying the service
   Eg: "service:abc123"
2. `sector`: Sector or root namespace
   Eg: "main", "web", or "background"
3. `weight`: Routing weight
   Eg: 1 (Before shutting down, services generally will emit a packet with this set to 0)
4. `sendInterval`: Interval in milliseconds
   Eg: 5000
5. `uri`: URI for service requests
   Eg: "beepish+tls://172.18.0.1:3000"
6. `envelopes_and_v4actions`
   This field is an array of any number of (string) envelope types (eg "json", "jsonstore") which are supported by the service.
   For v4 packets, after the last (string) envelope type, there will be a json object containing field-wise rle encoding of the actions supported by the service.
   Eg: `["json","jsonstore","extdirect",{"vmin":0,"vmaj":4,"acsec":["web"],"acname":["csv"],"acver":[1],"acenv":["web"],"acflag":["noauth,t900"],"acns":["Download.Report"]}]`

   After un-rle'ing: this will be converted into something like:
   Sector: web, Namespace: Download.Report, name: csv, Flags: noauth,t900, Version: 1, envelopes: web
   Notes:

   - vmin isn't used for anything
   - acenv seems to override the envelopes given prior to the json object (in this case: json, jsonstore, extdirect)
   - acsec seems to override the sector field of the packet, allowing actions to be offered in a variety of sectors

7. `v3actions`: v3 format Offered actions (array)
   An older format of representing actions. Unlike v4actions, this is not rle, and it's also only capable of representing actions which use the default sector from the field 2, and the envelopes portion of envelopes_and_v4actions. actions which apply to different sectors or envelopes from the default must be represented in v4actions.

   In theory, new implementations should be using only v4actions, but for some oddball reason, some ServiceInfo packets use both formats in the same packet

8. `timestamp`: Generation timestamp (double)

Example packet json:

```
[3,"payment:WJ24i9qkpIMP4c6jqOXnvL2q","main",1,5000,"beepish+tls://172.18.0.9:30309",["json","jsonstore","extdirect",{"vmin":0,"vmaj":4,"acsec":["web"],"acname":["handle_pj_webhook"],"acver":[1],"acenv":["web"],"acflag":["noauth"],"acns":["Edi.Payment.Module.PayJunction"]}],[["Payment.Config",["discover_devices",""],["setup_webhooks",""]],["Payment.CreditCard",["fetch","read"],["retire","destroy"]],["Payment.Series",["auth",""],["cancel_device_request",""],["charge",""],["credit",""],["query",""],["record",""],["request_device_payment",""],["save_card",""],["settle",""],["void",""]],["Payment.Transaction",["cancel",""],["capture",""],["list","read"],["void",""]],["_meta",["documentation","noauth"]]],1720724098.60031]
```
