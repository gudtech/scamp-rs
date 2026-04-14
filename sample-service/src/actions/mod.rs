// Each submodule maps to a SCAMP namespace hierarchy.
// The module path auto-derives the namespace via module_path!():
//   actions::api::status       → Api.Status
//   actions::constant::ship::carrier → Constant.Ship.Carrier
//   actions::order::shipment   → Order.Shipment
//   etc.

pub mod api;
pub mod background;
pub mod constant;
pub mod order;
pub mod scamp_rs_test;
