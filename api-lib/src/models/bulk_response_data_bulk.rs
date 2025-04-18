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
pub struct BulkResponseDataBulk {
    /// The list of Transaction ids that were created.
    #[serde(rename = "transaction_ids")]
    pub transaction_ids: Vec<String>,
    /// If any Transactions were not created because they had an `import_id` matching a transaction already on the same account, the specified import_id(s) will be included in this list.
    #[serde(rename = "duplicate_import_ids")]
    pub duplicate_import_ids: Vec<String>,
}

impl BulkResponseDataBulk {
    pub fn new(transaction_ids: Vec<String>, duplicate_import_ids: Vec<String>) -> BulkResponseDataBulk {
        BulkResponseDataBulk {
            transaction_ids,
            duplicate_import_ids,
        }
    }
}

