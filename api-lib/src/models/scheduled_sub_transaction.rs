/*
 * YNAB API Endpoints
 *
 * Our API uses a REST based design, leverages the JSON data format, and relies upon HTTPS for transport. We respond with meaningful HTTP response codes and if an error occurs, we include error details in the response body.  API Documentation is at https://api.ynab.com
 *
 * The version of the OpenAPI document: 1.72.1
 * 
 * Generated by: https://openapi-generator.tech
 */

use crate::models;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduledSubTransaction {
    #[serde(rename = "id")]
    pub id: uuid::Uuid,
    #[serde(rename = "scheduled_transaction_id")]
    pub scheduled_transaction_id: uuid::Uuid,
    /// The scheduled subtransaction amount in milliunits format
    #[serde(rename = "amount")]
    pub amount: i64,
    #[serde(rename = "memo", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]
    pub memo: Option<Option<String>>,
    #[serde(rename = "payee_id", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]
    pub payee_id: Option<Option<uuid::Uuid>>,
    #[serde(rename = "category_id", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]
    pub category_id: Option<Option<uuid::Uuid>>,
    /// If a transfer, the account_id which the scheduled subtransaction transfers to
    #[serde(rename = "transfer_account_id", default, with = "::serde_with::rust::double_option", skip_serializing_if = "Option::is_none")]
    pub transfer_account_id: Option<Option<uuid::Uuid>>,
    /// Whether or not the scheduled subtransaction has been deleted. Deleted scheduled subtransactions will only be included in delta requests.
    #[serde(rename = "deleted")]
    pub deleted: bool,
}

impl ScheduledSubTransaction {
    pub fn new(id: uuid::Uuid, scheduled_transaction_id: uuid::Uuid, amount: i64, deleted: bool) -> ScheduledSubTransaction {
        ScheduledSubTransaction {
            id,
            scheduled_transaction_id,
            amount,
            memo: None,
            payee_id: None,
            category_id: None,
            transfer_account_id: None,
            deleted,
        }
    }
}

