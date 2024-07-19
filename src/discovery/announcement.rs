/// ServiceInfo is a struct that represents a service announcement
/// Including the address of the service, and the actions it offers
/// as well as the certificate and signature of the announcement
pub struct Announcement {
    version: u32,
    actions: Vec<Action>,
    address: String,
    certificate: String,
    signature: String,
}
struct Action {
    name: String,
    flags: Vec<String>,
    version: u32,
}

use serde::{Deserialize, Serialize};

// #[derive(Deserialize, Serialize, Debug)]
// struct Raw {
//     vmaj: u32,
//     ident: String,
//     #[serde(rename = "v3sector")]
//     sector: String,
//     wgt: u32,
//     intvl: u32,
//     uri: String,
//     #[serde(rename = "v3env")]
//     env: Vec<String>,
//     #[serde(rename = "v3actions")]
//     actions: Vec<Vec<serde_json::Value>>,
//     ts: f64,
// }

// #[derive(Error, Debug)]
// pub enum AnnouncementError {
//     #[error("Invalid announcement format")]
//     InvalidFormat,
//     #[error("Invalid JSON: {0}")]
//     InvalidJson(#[from] serde_json::Error),
//     #[error("Invalid action data")]
//     InvalidActionData,
// }

// impl Announcement {
//     pub fn parse_announcement(announcement: &str) -> Result<Self, AnnouncementError> {
//         let parts: Vec<&str> = announcement.split("\n\n").collect();
//         if parts.len() != 3 {
//             return Err(AnnouncementError::InvalidFormat);
//         }

//         let json_blob = parts[0];
//         let cert_pem = parts[1];
//         let sig_base64 = parts[2];

//         let raw: Raw = serde_json::from_str(json_blob)?;
//         let actions = Self::parse_actions(&raw.actions)?;

//         Ok(Announcement {
//             version: raw.vmaj,
//             actions,
//             address: raw.uri,
//             certificate: cert_pem.to_string(),
//             signature: sig_base64.to_string(),
//         })
//     }

//     fn parse_actions(
//         raw_actions: &[Vec<serde_json::Value>],
//     ) -> Result<Vec<Action>, AnnouncementError> {
//         let mut actions = Vec::new();

//         for action_group in raw_actions {
//             if action_group.is_empty() {
//                 continue;
//             }
//             let namespace = action_group[0]
//                 .as_str()
//                 .ok_or(AnnouncementError::InvalidActionData)?
//                 .to_string();
//             for action_data in action_group.iter().skip(1) {
//                 let action_arr = action_data
//                     .as_array()
//                     .ok_or(AnnouncementError::InvalidActionData)?;
//                 if action_arr.len() < 2 {
//                     return Err(AnnouncementError::InvalidActionData);
//                 }
//                 let name = format!(
//                     "{}.{}",
//                     namespace,
//                     action_arr[0]
//                         .as_str()
//                         .ok_or(AnnouncementError::InvalidActionData)?
//                 );
//                 let flags = action_arr[1]
//                     .as_str()
//                     .ok_or(AnnouncementError::InvalidActionData)?
//                     .split(',')
//                     .filter(|s| !s.is_empty())
//                     .map(String::from)
//                     .collect();
//                 let version = action_arr.get(2).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
//                 actions.push(Action {
//                     name,
//                     flags,
//                     version,
//                 });
//             }
//         }

//         Ok(actions)
//     }

//     fn unrle<T: Clone>(rle: &[T]) -> Vec<T> {
//         let mut result = Vec::new();
//         for item in rle {
//             if let Some((count, value)) = item.as_array().and_then(|arr| {
//                 if arr.len() == 2 {
//                     Some((arr[0].as_u64()?, arr[1].clone()))
//                 } else {
//                     None
//                 }
//             }) {
//                 result.extend(std::iter::repeat(value).take(count as usize));
//             } else {
//                 result.push(item.clone());
//             }
//         }
//         result
//     }
// }

// ... existing code ...

#[cfg(test)]
mod tests {
    #[test]
    fn rough_parts() {
        let parts: Vec<&str> = sample_announcement.split("\n\n").collect();
        assert_eq!(parts.len(), 3, "Expected 3 parts in the announcement");
    }

    // #[test]
    // fn test_parse_announcement() {
    //     let service_info = ServiceInfo::parse_announcement(sample_announcement);
    //     assert_eq!(service_info.address, "172.18.0.7");
    // }

    const sample_announcement: &str = r#"[3,"mainapi:4HaM4TN5IVSLNfqhERfKvsVu","main",1,5000,"beepish+tls://172.18.0.7:30201",["json","jsonstore","extdirect",{"acflag":[[3,"noauth"],[3,""]],"vmin":0,"vmaj":4,"acenv":[[3,"web"],[3,"json,jsonstore,extdirect"]],"acsec":[[3,"web"],[3,"taxmodule"]],"acver":[[6,1]],"acname":["journalentries","csv","pdf",[3,"calculate"]],"acns":["Download.Financials",[2,"Download.PO"],"Flat","TaxJar","VAT"]}],[["API.Documentation",["fetch_tree","noauth,read"]],["API.Status",["health_check",""]],["API",["clientInitiatedLog",""],["getNodeList",""]],["Config.ACL.Privs",["read","read"]],["Config.ACL.Roles",["read","read"]],["Config.Account.Param",["fetch","read","2"],["read","read"],["update","update"],["update","update","2"]],["Config.Account.RoutingParam",["getOrderMode",""],["read","read"],["test",""],["test","","2"],["update","update"]],["Config.Account",["read","read"],["update","update"]],["Config.Address",["fetch","read"],["get",""],["verifyAndSave",""]],["Config.Attribute.Dependency",["list","read"]],["Config.Attribute.Map",["list","read"],["save","update"]],["Config.Attribute.MetaFields",["create","create"],["list","read"],["update","update"]],["Config.Attribute.ValTree",["list","read"],["update","update"]],["Config.Attribute.Value",["list","read"]],["Config.Attribute",["create","create"],["fetch","read"],["list",""],["update","update"]],["Config.Box",["create","create"],["fetch","read"],["print_label",""],["update","update"]],["Config.Brand",["create","create"],["get",""],["list","read"],["update","update"]],["Config.Channel.CA",["get","read"],["set",""],["test",""]],["Config.Channel.Definition",["fetch","read"]],["Config.Channel.InventoryLocation",["create","create"],["fetch","read"],["update","update"]],["Config.Channel.MagentoExtension",["get","read"],["set",""],["test",""]],["Config.Channel",["create","create"],["fetch","read"],["update","update"]],["Config.Container.Range",["assertContainers",""],["create","create"],["fetch","read"],["update","update"]],["Config.Device",["email",""],["fetch","read"],["printCommandLabel",""],["register",""],["test",""]],["Config.DeviceProxy",["get",""]],["Config.Email.Profile",["fetch","read"]],["Config.Employee",["fetch","read"]],["Config.Facility.Zone",["create","create"],["fetch","read"],["update","update"]],["Config.Facility",["create","create"],["fetch","read"],["get",""],["update","update"]],["Config.Feed.ErrorRecords",["fetch","read"]],["Config.Feed.Module",["list","read"]],["Config.Feed.Schema",["fetch","read"],["update","update"]],["Config.Feed.Test",["getFields","read","2"],["testTemplates","","3"]],["Config.Feed",["create","create"],["fetch","read"],["getErrorInfo",""],["poke",""],["serialize",""],["update","update"]],["Config.FraudFactorMap",["create","create"],["fetch","read"],["update","update"]],["Config.Fulfillment.Actor",["fetch","read"],["update","update"]],["Config.Fulfillment.ActorRole",["fetch","read"],["update","update"]],["Config.Inventory.AdjustmentReason",["create","create"],["fetch","read"],["update","update"]],["Config.MapTable.Line",["create","create"],["fetch","read"],["update","update"]],["Config.MapTable",["create","create"],["fetch","read"],["update","update"]],["Config.OldFeed",["create","create"],["fetch","read"],["update","update"]],["Config.Order.CancelReason",["create","create"],["fetch","read"],["update","update"]],["Config.Order.DiscountReason",["create","create"],["fetch","read"],["update","update"]],["Config.Order.ItemOption",["fetch","read"]],["Config.PO.RoutingGuide",["fetch","read"],["load",""],["save",""]],["Config.POType",["create","create"],["fetch","read"],["update","update"]],["Config.Param",["getAll",""]],["Config.Payment.Account",["create","create"],["fetch","read"],["update","update"]],["Config.ReturnItemReason",["create","create"],["fetch","read"],["update","update"]],["Config.Serials",["fetch","read"],["set","create,update"]],["Config.Ship.Account",["create","create"],["fetch","read"],["subscriptionConfiguration",""],["update","update"]],["Config.ShipService.CarrierClassMapping",["create","create"],["fetch","read"],["update","update"]],["Config.ShipService.ChannelMapping",["create","create"],["fetch","read"],["update","update"]],["Config.ShipService.MappingRules",["create","create"],["fetch","read"],["update","update"]],["Config.ShipService",["create","create"],["fetch","read"],["update","update"]],["Config.Subchannel",["fetch","read"],["update","update"]],["Config.Task",["fetch","read"],["reset",""],["update","update"]],["Config.TaxNexus",["create",""],["fetch","read"],["update",""]],["Config.Terminal.DevicePrefs",["create","create","2"],["delete","destroy","2"],["fetch","read"],["fetch","read","2"],["save","update"],["update","update","2"]],["Config.Terminal",["create","create"],["fetch","read"],["getMyTerminalInfo",""],["hasPaymentDevice",""],["reauth",""],["update","update"]],["Config.User.ACL",["create","create"],["destroy","destroy"],["read","read"]],["Config.User.Credentials",["createApiKey",""],["printBadge",""],["read","read"],["setCredentialActive",""],["setLoginData",""]],["Config.User",["create","create"],["read","read"],["update","update"]],["Config.Vendor.Contact",["create","create"],["fetch","read"],["update","update"]],["Config.Vendor",["create","create"],["fetch","read"],["update","update"]],["Constant.AppVersion",["fetch","read"]],["Constant.Country",["fetch","read"]],["Constant.Enum",["allEnums",""],["list","read"]],["Constant.FraudFactor",["fetch","read"]],["Constant.Param",["set_params",""]],["Constant.Payment.Terms",["create",""],["fetch","read"]],["Constant.Ship.Carrier",["fetch","read"]],["Constant.Ship.CarrierClass",["fetch","read"]],["Constant.Ship.PackagingType",["fetch","read"]],["Constant.Ship.Processor",["fetch","read"]],["Constant.Ship.Speed",["fetch","read"]],["Constant.State",["fetch","read"]],["Constant.TaxModule",["fetch","read"]],["Constant.Template",["fetch","read"]],["Constant.TimeZone",["fetch","read"]],["Customer.Address",["fetch","read"],["get",""],["retire","destroy"],["verifyAndSave",""]],["Customer.Order.Email",["generate_body",""],["send_with_body",""]],["Customer.Order.FraudRisk",["list","read"]],["Customer.Order.Item",["create","create"],["list","read"],["retire","destroy"],["update","update"]],["Customer.Order.RMA",["create",""],["list","read"],["update","update"]],["Customer.Order.Report",["list","read"]],["Customer.Order.Return",["create",""],["submit",""]],["Customer.Order.RoutingGroup",["fetch","read"],["regroup",""]],["Customer.Order.Shipment.Item",["cancel",""],["fetch","read"],["reorder",""]],["Customer.Order.Shipment.Package",["fetch","read"],["reprint",""],["void",""]],["Customer.Order.Shipment.Picks",["list","read"]],["Customer.Order.Shipment.Report",["list","read"],["list","read","2"]],["Customer.Order.Shipment.SkuFrequencyReport",["list","read"]],["Customer.Order.Shipment.SkuReport",["list","read"]],["Customer.Order.Shipment",["get",""],["list","read,t300"]],["Customer.Order",["applyStoreCredit",""],["authCard",""],["authCheck",""],["authGiftCard",""],["authMoneyOrder",""],["authTradeCredit",""],["cancel",""],["capTradeCredit",""],["chargeCash",""],["chargeGiftCard",""],["creditPayment",""],["emailInvoice",""],["fetch","read"],["hold",""],["make",""],["markReadyForPickup",""],["printInvoice",""],["printReceipt",""],["push",""],["push","","2"],["recCredit",""],["recordExternalCardCharge",""],["recordExternalThirdPartyPayment",""],["recordPaymentEvent",""],["requestDevicePayment",""],["submit","create"],["submit","create","2"],["update","update"]],["Customer.RMA.Item",["fetch","read"],["update","update"]],["Customer.RMA",["create","create"],["fetch","read"],["update","update"]],["Customer.StoreCredit.Transaction",["list","read"]],["Customer.StoreCredit",["adjust",""]],["Customer",["create","create"],["fetch","read"],["printStoreCredit",""],["update","update"]],["Directory.BackgroundJob",["fetch","read"],["update","update"]],["Facility.CashTray",["adjustCash",""],["count",""],["create","create"],["fetch","read"],["print",""],["update","update"]],["Facility.CashTraySession",["close",""],["fetch","read"],["open",""],["report",""],["report","","2"],["update","update"]],["Fulfillment.ASN",["acknowledge",""],["fetch","read"]],["Fulfillment.Package",["create","create"],["fetch","read"],["populate_rates_for_params",""],["rate_offers",""],["update",""]],["Fulfillment.Ship.Session",["fetch","read"],["getStats",""],["has_close_documents",""],["open","create"],["reprint_close_documents",""],["update","update"]],["Fulfillment.Shipment",["CancelPendingTransmit",""],["MarkPendingTransmit",""],["MarkTransmitted",""],["complete",""],["complete_with_packages",""],["fetch","read"],["get",""]],["Fulfillment.Wave.Pick",["ack",""],["fetch",""],["next",""],["notfound",""]],["Fulfillment.Wave",["bulkPrint",""],["bulkShipmentAdd",""],["close",""],["consolidatedPrint",""],["consolidatedShipmentAdd",""],["create",""],["exportPickList",""],["list","read"],["print","t300"],["retransmit",""],["setPicksClosed",""],["setPicksTransmitted",""],["shipmentAdd",""]],["Integration.Channel",["register",""]],["Inventory.Container.Contents",["list","read"]],["Inventory.Container",["bulkAssert",""],["create",""],["createTote",""],["deactivate",""],["get",""],["list",""],["move",""],["print",""],["search","read"],["update","update"]],["Inventory.Count",["AddContainers",""],["LogItemCount",""],["NextItemCountRequest",""],["create","create"],["fetch","read"],["update","update"]],["Inventory.Count.Report.Progress",["list","read"]],["Inventory.Count.Report.User",["fetch","read"]],["Inventory.ExternalLot",["create","create"],["fetch","read"],["update","update"]],["Inventory.Lot",["create",""],["reprint",""],["search","read"],["update","update"],["void",""]],["Inventory.Quantity.Adjustment",["fetch","read"]],["Inventory.Quantity",["adjust",""],["search","read"]],["Inventory.Receive",["createRecItem",""],["getWeight",""],["receive",""],["reprint",""]],["Inventory.Transfer.Batch",["cancel",""],["create","create"],["list","read"],["update","update"]],["Inventory.Transfer.Item",["ack",""],["create","create"],["list","read"],["next",""],["notfound",""],["update","update"]],["Media.Map",["fetch","read"],["update","update"]],["Media.Work",["fetch","read"],["get",""],["update","update"]],["Media",["formats",""],["mapWork",""],["registerFile",""],["registerWork",""],["workInfo",""]],["Nav.HomeFeed",["fetch","read"]],["Nav.Shortcuts",["fetch","read"],["update","update"]],["Nav",["list","read"]],["Notes",["add",""],["create","create"],["create","create","2"],["list","read"]],["Po.Item",["create","create"],["itemsByGroup","read"],["retire","destroy"],["update","update"]],["Po.ItemGroup",["create","create"],["getAttributeColumns",""],["list","read"],["update","update"]],["Po.Manifest.Item",["fetch","read"],["fetch","read","2"],["fetch","read","3"],["get",""]],["Po.Manifest",["fetch","read"]],["Po.Note",["add",""],["list","read"]],["Product.APETree",["fetch","read"],["update","update"]],["Product.Association",["create","create"],["fetch","read"],["update","update"]],["Product.ExternalSku.Quant",["read","read"]],["Product.ExternalSku",["create","create"],["search","read"],["update","update"],["update","","2"]],["Product.Family.Locator",["locate","read"]],["Product.Family",["create",""],["get",""],["save",""],["search","read"],["update","update"]],["Product.FamilyIdent",["search","read"]],["Product.Ident",["assert",""],["get",""],["search","read"]],["Product.PhotoSample",["choose",""],["list","read"]],["Product.Po",["create","create"],["create","create","2"],["fetch","read"],["fetch","read","2"],["markReady",""],["markSubmitted",""],["update","update"],["update","update","2"]],["Product.PriceHistory",["read","read"]],["Product.PriceHistoryAttr",["read","read"]],["Product.Sku.Barcode",["add","create"],["delete","destroy"],["fetch","read"]],["Product.Sku.MediaUtil",["getRepSkus",""]],["Product.Sku",["addMediaMapping",""],["assertByAttrs",""],["get",""],["getAttributeData",""],["getLegacyLot",""],["inventory_poke",""],["printLabel",""],["queryMedia",""],["reserve",""],["retire",""],["save","create,update"],["search","read"]],["Receive.Piece",["create","create"],["fetch","read"],["move",""],["reprint",""],["update","update"]],["Receive.Shipment",["createAndPrint",""],["fetch","read"],["update","update"]],["User",["getInfo",""],["getInfo","","2"],["getPrivs",""],["reportError",""]],["Utility.Address",["autoComplete",""]],["Utility.Dashboard",["inventory_actions_by_user_today",""],["order_count_today",""],["order_statuses",""],["order_summary",""],["order_summary","","2"],["orders_completed_by_hour",""],["orders_today_by_channel",""],["orders_today_by_channel_subchannel",""],["orders_today_by_hour",""],["pos_orders_today_by_zone",""],["queue_depth_history",""],["top_skus_today",""]],["Utility.EventLog",["annotate",""],["createEvent",""],["fetch","read"]],["Utility.EventLogSummary",["fetch","read"],["resolve",""]],["Utility.Tag",["search","read"]],["Vendor.ASN",["enable","update"]],["Vendor.PO",["Complete",""],["CompleteDS",""],["MarkAccepted",""],["MarkSubmitted",""],["SubmitPartialTracking",""],["UpdateEstShipDate",""],["fetch","read"]],["_meta",["documentation","noauth"]]],1720724094.61916]

-----BEGIN CERTIFICATE-----
MIIE+zCCAuOgAwIBAgIJAL7Cq628mecpMA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNV
BAMMCSBiZ3dvcmtlcjAeFw0xODA3MTEyMzM0NDhaFw0yODA3MDgyMzM0NDhaMBQx
EjAQBgNVBAMMCSBiZ3dvcmtlcjCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoC
ggIBAOTEAjyMb1zQI5eswQOcSQHdQhBwZMu/PbJHbKnqhkGYgyWMasb2WrbU0N2s
L/dIxBPWisWwbJNiyiyrfk0OZapqgHmzC6Il3UqfLw9bjpyjm+3vcJYtOLs/NqcZ
MSJj//XX3Bg1u1Gd6gH0vQF6DHdtbEo5UMHxzD7eAHZnoFQQpJ0ZXIRKBVmTd4XW
k3DuJTz1lygkdXjNmtAsNFquKz2XyWtot/Bl+QsAGC73rITUxQ7cZjUW8W/26IU+
yJEIrt87xz/niTXqVCha7EjzwSeqUSabZb67bqBl1oes5+FqBXrKhioFBsgrcqY5
Gp4YC6jdGu68h3zxRLfIGwy7PCHN0Mn27L5JkG7La47eBOUW/BfZbAlC22DkKMHN
FOiI9Hma8c4NmwSoYxds6DAN0SgTEr0xAFcQ7bBaLh4tQYGNaf0kTHQr92f2hrVg
or88N5QYNecTvBJL3ndVdnVRtzIc4Cy+NenNswy/49d4/EyLFJBHDFDlcF4NdRsE
iwVBFdFZf71yEa4qObKdsmJKKdxeg71B/FzwGmfpsegKYR/lIo9pHWYiUygH2EHj
MQinykJb0D7xOx566vSRnEaVs2d94VoWEuf8cJDk+Xa1tSXdbB2R6r0wYJVg3MXx
qV1tqDGwUY14je6r3qmFLjXFSQiEGhqhvp25H8BciQJVowrLAgMBAAGjUDBOMB0G
A1UdDgQWBBSCkvolbWOFtnfbU/Qsn4UuonDDGTAfBgNVHSMEGDAWgBSCkvolbWOF
tnfbU/Qsn4UuonDDGTAMBgNVHRMEBTADAQH/MA0GCSqGSIb3DQEBCwUAA4ICAQBm
JE/PvK/wH5QRXgRuQsyAJbY8ShCp+oEW+zwkGlNfMANoQHTIYlYS87UmwrHBvqTy
FF6hJJWwDSVKktdN7sHJQn7eMvRC6bN9wfE2XWslBsOnLEmwQrv7MZFMW0NGBS0z
UXb7kCRqzgH4xdCLeIvEVNUa6p+0WuZKN1VdxEu5iRJp3C5GaQ4z274xScmicTBj
KYG1TTUD7JQhu+DetAWsq3qzeQdbrPFdgo7MCEc/RQKIo/qy8liqfnYIFi1/BZ7i
WiUTcwyNRxxdsO11o8+cVMEUdE/ZjN7694qn9t884oiF0/OAfUyojifcQnnbt6t3
xUpi1SR4V1WbDJz9epzC75lIqtXOIS0tDkGIA1fCm7HvbvIvrXzWbZsKhx+6Fhd7
LriFFnB0EDiC11+WR7x5B2E58bEbxbZbMD+6XG30MAcvwVncK9zoIa7OVfOtWc+z
vkIlSCiY18E05KpI4q2VOngjvEoZ6nVESq3H+RshmjtocB1k90PEeCGwYtYyJpCd
5G/hXTv+B++f6UogSecNQ+EhkcKQG76pixDpSMycyIAPnkxGLNTM1MDbNM6bZncr
cO94HLUE0z7abEB5JzPL50dHmTeDLT5crAjegyh5oAbwXLBjzxMWriqD64usZFbl
gYTbAnfiSomeSgM0pBS708CXGPokvsakZp4VgGdNRQ==
-----END CERTIFICATE-----

m/1lF3oK1i9R0A/BFo4/eFGdNtPb62BS/IrgrSjRfI2AbHiUU+r0Ko3PDULUbrWRGfSGHruKsmUQ
qhEKyQftk+NvQ/2M6KT6IZjxD/QW0EQ9QlGxRXfWm+U57/v7thbK2z5opa4vzSgotL9olEbGZFu9
AeFD9z3FnvHei8sQM4YVJcjLGT2662PB2N/C+Z5erfbZAa8xpeRWv6CaU4IP3VGQ+6JAO7drD2eK
o4fUfzm5lsCaFTkSa4VEG25kHBqWWNnNQxM1SgNGyCGAPKtXv/k7Hkv/ZbpyTjp2Vq5leEJsSqRo
XTklPR/NaoaVyZBTfesY7oSELODxZ/Yz6efSQmsAmTBTxbZDUcjqtB71XCuaYuPkDN5LBysQjeJm
FeOSz8vWzVIj4MkVMcjczuDon3kLvR7+TjhEf51EuxLJeRgvbulCC6SDbC3NRlUGaZC3pZBoXA9j
OlghwImlrRe4p/DGfQop+uwljX9ktABEcj7vXLsyDLdAz1ev44EC7Tu/blJqp9cR6rhs/LLDLlo4
aI7z61kJwcT2WnRJsKiNo0lcYOW2BHBzGYf+9AO1+qn5p5BP5PgFpc4C/MY1K7OO8vW69MaS4hzr
vM1lP/4Ji31bON9bn+KDDzNrDmHjks/28qWqpKJGCUlc5WT1s8PhRWUaw0tK72k/InJHiDY6jM4=
"#;
}

// existing perl code

// package GTSOA::Discovery::ServiceInfo;

// # This roughly corresponds to lib/handle/service.js but one big difference is
// # that it is _immutable_

// use Moose;
// use namespace::autoclean;
// use Try::Tiny;

// use JSON::XS;
// use MIME::Base64;
// use Digest;
// use File::Slurp 9999.14;
// use Crypt::X509;
// use Crypt::OpenSSL::RSA;
// use GTSOA::Config;

// has ref           => (is => 'bare', required => 1, isa => 'HashRef');
// has json_blob     => (is => 'ro', required => 1, isa => 'Str');
// has cert_pem      => (is => 'ro', required => 1, isa => 'Str');
// has sig_base64    => (is => 'ro', required => 1, isa => 'Str');
// has orig          => (is => 'ro', required => 1, isa => 'Str');

// has fingerprint   => (is => 'ro', isa => 'Str',  lazy_build => 1);
// has verified      => (is => 'ro', isa => 'Bool', lazy_build => 1);
// has offerings     => (is => 'ro', isa => 'HashRef[ArrayRef]', lazy_build => 1);
// has expires       => (is => 'rw', isa => 'Num');

// has _unauth_warned => (is => 'bare', init_arg => undef);

// sub version       { $_[0]{ref}{vmaj} }
// sub worker_ident  { $_[0]{ref}{ident} }
// sub weight        { $_[0]{ref}{wgt} }
// sub send_interval { $_[0]{ref}{intvl} / 1000 }
// sub address       { $_[0]{ref}{uri} }
// sub timestamp     { $_[0]{ref}{ts} }
// # sector, envelopes, can_envelope, action_list REMOVED

// sub parse_announcement {
//     my ($self, $blob) = @_;

//     my ($json, $cert_pem, $sig) = split /\n\n/, $blob;

//     my $data = decode_json($json);
//     my $ref;

//     if (ref($data) eq 'ARRAY') {
//         # V3/V3.5 packet

//         die("invalid number of elements in v3 packet\n") if @$data != 9;
//         die("unknown v3 protocol version: $data->[0]\n") if $data->[0] != 3;

//         if (ref($data->[6]) eq 'ARRAY' && @{$data->[6]} && ref($data->[6][-1]) eq 'HASH') {
//             $ref = pop @{$data->[6]};
//         }
//         else {
//             $ref = {};
//         }

//         @$ref{qw/ vmaj ident v3sector wgt intvl uri v3env v3actions ts /} = @$data;
//     }
//     else {
//         $ref = $data;
//     }

//     $self->new( ref => $ref, json_blob => $json, cert_pem => $cert_pem, sig_base64 => $sig, orig => $blob );
// }

// sub _unpem {
//     my ($text) = @_;

//     decode_base64( join '', grep { !/^--/ } split /\n/, $text );
// }

// sub _pem {
//     my ($tag, $data) = @_;

//     return "-----BEGIN $tag-----\n" . encode_base64($data) . "-----END $tag-----\n";
// }

// sub _build_fingerprint {
//     my ($self) = @_;

//     my $hash   = uc Digest->new('SHA-1')->add(_unpem($self->cert_pem))->hexdigest;

//     $hash =~ s/(..)(?!$)/$1:/g;
//     $hash;
// }

// sub _build_verified {
//     my ($self) = @_;

//     my $ok = 0;
//     try {
//         my $x509      = Crypt::X509->new( cert => _unpem($self->cert_pem) );
//         die $x509->error if $x509->error;

//         my $verify_key = Crypt::OpenSSL::RSA->new_public_key( _pem 'RSA PUBLIC KEY', $x509->pubkey );

//         $verify_key->use_sha256_hash;
//         $verify_key->use_pkcs1_oaep_padding;

//         $ok = $verify_key->verify( $self->json_blob, decode_base64($self->sig_base64) );
//     } catch {
//         GTSOA::Logger->error("Unable to verify signature for ".$self->worker_ident." $_");
//     };

//     $ok;
// }

// my %authorized_keys;
// my $ak_mtime = -2**48;
// my $path = GTSOA::Config->val('bus.authorized_services');

// sub _get_authorized_keys {
//     my $mt = (stat $path)[9];

//     if ($mt != $ak_mtime) {
//         my @lines = read_file($path, binmode => ':utf8');
//         %authorized_keys = ();
//         $ak_mtime = $mt;

//         for my $line (@lines) {
//             $line =~ s/#.*//;
//             $line =~ s/\s+$//;
//             $line =~ s/^\s+//;

//             next unless length $line;

//             my ($fingerprint, $toks) = $line =~ /^(\S*)\s*(.*)$/;
//             my @toks = map { quotemeta } split /\s*,\s*/, $toks;
//             for (@toks) { if (/:/) { s/:ALL$/:.*/ } else { $_ = "main:$_" } }
//             my $tok_rx = join('|', @toks);

//             $authorized_keys{ $fingerprint } = qr/^(?:$tok_rx)(?:\.|$)/i;
//         }
//     }

//     return \%authorized_keys;
// }

// sub authorized {
//     my ($self, $sector, $action) = @_;

//     # every service is competent to requests about itself
//     return 1 if $action =~ /^_meta\./;

//     # nice try
//     return 0 if $action =~ /:/ || $sector =~ /:/;

//     if (!$self->verified) {
//         GTSOA::Logger->error('Service does not have a valid signature ' . $self->fingerprint);
//         return 0;
//     }

//     my $rx = $self->_get_authorized_keys->{ $self->fingerprint };
//     if (!$rx) {
//         #if (!$self->{_unauth_warned}) { GTSOA::Logger->error('Unauthorized service ' . $self->fingerprint); }
//         #$self->{_unauth_warned} = {};
//         return 0;
//     }
//     $self->{_unauth_warned} ||= {};
//     return 1 if "$sector:$action" =~ /$rx/;
//     if (!$self->{_unauth_warned}{$action}++) {
//         # GTSOA::Logger->error('Service '.$self->fingerprint.' not authorized to provide '.$action);
//     }
//     return 0;
// }

// sub _build_offerings {
//     my ($self) = @_;

//     my %map;

//     if ($self->{ref}->{v3actions}) {
//         my $sector = $self->{ref}->{v3sector};
//         my $envelopes = $self->{ref}->{v3env};

//         for my $nsinfo (@{$self->{ref}->{v3actions}})
//         {
//             my ($ns, @actions) = @$nsinfo;

//             for my $act (@actions)
//             {
//                 my $aname = $ns . '.' . $act->[0];
//                 my $vers  = $act->[2] || 1;
//                 my $info  = [ $aname, $vers, [ $act->[1] ? (split /,/, $act->[1]) : () ], $sector, $envelopes ];

//                 $map{ "\L$sector:$aname.v$vers" } = $info;

//                 for my $tag (@{ $info->[2] }) {
//                     # some tags define aliases
//                     $map{ "\L$sector:$ns._$tag.v$vers" } = $info if $tag =~ /^(?:create|read|update|destroy)$/;
//                 }
//             }
//         }
//     }

//     if ($self->{ref}->{acname}) {
//         my $actionL = __unrle('acname', $self->{ref}->{acname} || die("acname required"));
//         my $len = @$actionL;
//         my $actnsL = __unrle('acns', $self->{ref}->{acns} || die("acns required"), $len);
//         my $compatverL = __unrle('accompat', $self->{ref}->{accompat} || [[$len,1]], $len);
//         my $actverL = __unrle('acver', $self->{ref}->{acver} || [[$len,1]], $len);
//         my $actflagL = __unrle('acflag', $self->{ref}->{acflag} || [[$len,'']], $len);
//         my $actenvL = __unrle('acenv', $self->{ref}->{acenv} || die("acenv required"), $len);
//         my $actsecL = __unrle('acsec', $self->{ref}->{acsec} || die("acsec required"), $len);

//         while (@$actionL) {
//             my $action = shift @$actionL;
//             my $compatver = shift @$compatverL;
//             my $actver = shift @$actverL;
//             my $actflag = shift @$actflagL;
//             my $actenv = shift @$actenvL;
//             my $actsec = shift @$actsecL;
//             my $actns = shift @$actnsL;

//             my $info = [ "$actns.$action", $actver, [grep {$_} split /,/, $actflag], $actsec, [grep {$_} split /,/, $actenv] ];

//             next if $compatver != 1;

//             $map{"\L$actsec:$actns.$action.v$actver"} = $info;
//             for my $tag (@{ $info->[2] }) {
//                 # some tags define aliases
//                 $map{ "\L$actsec:$actns._$tag.v$actver" } = $info if $tag =~ /^(?:create|read|update|destroy)$/;
//             }
//         }
//     }

//     \%map;
// }

// sub __unrle {
//     my ($name, $rle, $len) = @_;

//     my @out;
//     for my $ent (@$rle) {
//         die ("$name array entry must be 2-element") if ref($ent) eq 'ARRAY' && @$ent != 2;
//         my ($ct, $obj) = ref($ent) eq 'ARRAY' ? @$ent : (1, $ent);

//         die ("invalid repeat count $name:$ct") if $ct ne (0+$ct) || $ct < 0;
//         die ("repeat count overflow $name") if $ct + @out > ($len // 1e5);
//         push @out, ($obj) x $ct;
//     }
//     die ("repeat count undeflow $name: ".@out) if defined($len) && @out != $len;

//     \@out;
// }

// sub can_do {
//     my ($self, $sector, $action, $version, $envelope) = @_;

//     my $blk = $self->offerings->{ "\L$sector:$action.v$version" } or return;  # check this FIRST to avoid costly crypto
//     $self->authorized( $sector, $action ) or return;
//     grep { $_ eq $envelope } @{$blk->[4]} or return;

//     my $timeout = GTSOA::Config->val('rpc.timeout', 75);
//     for (@{$blk->[2]}) { /^t(\d+)$/ and $timeout = $1 + 5 }

//     return { name => $blk->[0], version => $blk->[1], flags => $blk->[2], service => $self, timeout => $timeout };
// }

// __PACKAGE__->meta->make_immutable;

//
