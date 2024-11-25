use chrono;
use chrono::{serde::ts_milliseconds, DateTime, NaiveDate, NaiveDateTime, Utc};
use regex::Regex;
use serde::{de, Deserialize, Deserializer};
use sgmlish;

#[derive(Debug, Deserialize)]
struct Ofx {
    #[serde(rename = "BANKMSGSRSV1")]
    response: BankMessagesResponseV1,
}

#[derive(Debug, Deserialize)]
pub struct BankMessagesResponseV1 {
    #[serde(rename = "STMTTRNRS")]
    response: StatementTransactionResponse,
}

#[derive(Debug, Deserialize)]
pub struct StatementTransactionResponse {
    #[serde(rename = "STMTRS")]
    response: StatementResponse,
}

#[derive(Debug, Deserialize)]
struct StatementResponse {
    #[serde(rename = "BANKTRANLIST")]
    transaction_list: TransactionList,
}

#[derive(Debug, Deserialize)]
struct TransactionList {
    #[serde(rename = "STMTTRN")]
    transactions: Vec<OfxTransaction>,
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    struct YMDStringVisitor;

    impl<'de> de::Visitor<'de> for YMDStringVisitor {
        type Value = NaiveDate;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a datetime string in the format %Y%m%d%H%M%S%.3f")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            NaiveDateTime::parse_from_str(v, "%Y%m%d%H%M%S%.3f")
                .map(|dt| dt.date())
                .map_err(|_| E::custom(format!("Failed to parse datetime: {}", v)))
        }
    }

    deserializer.deserialize_str(YMDStringVisitor)
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum TransactionKind {
    DEBIT = 1,
    CREDIT = 2,
    OTHER = 3,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct OfxTransaction {
    #[serde(rename = "TRNTYPE")]
    pub transaction_kind: TransactionKind,

    #[serde(rename = "DTPOSTED", deserialize_with = "deserialize_datetime")]
    pub date_posted: NaiveDate,

    #[serde(rename = "TRNAMT")]
    pub amount: f64,

    #[serde(rename = "NAME")]
    pub name: Option<String>,

    #[serde(rename = "MEMO")]
    pub memo: Option<String>,
}

// Just assume that the XML portion extends to the end of the file
fn get_ofx_block(file_contents: &str) -> Option<&str> {
    let re = Regex::new("<OFX>").unwrap();
    let m = re.find(file_contents)?;
    Some(&file_contents[m.start()..])
}

pub fn parse(file_contents: &str) -> Result<Vec<OfxTransaction>, sgmlish::Error> {
    let xml = get_ofx_block(file_contents).unwrap();
    let sgml = sgmlish::Parser::builder().uppercase_names().parse(xml)?;
    let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;
    let result = sgmlish::from_fragment::<Ofx>(sgml)?;
    Ok(result
        .response
        .response
        .response
        .transaction_list
        .transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;

    const SAMPLE: &str = "\
        OFXHEADER:100\
        DATA:OFXSGML\
        VERSION:102\
        SECURITY:NONE\
        ENCODING:USASCII\
        CHARSET:1252\
        COMPRESSION:NONE\
        OLDFILEUID:NONE\
        NEWFILEUID:NONE\
        \
        <OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO<MESSAGE>Authentication \
        Successful.</STATUS><DTSERVER>20241120170806.513[-5:EST]<LANGUAGE>ENG<FI><ORG>Tangerine\
        <FID>12345</FI><INTU.BID>12345</SONRS></SIGNONMSGSRSV1><BANKMSGSRSV1><STMTTRNRS><TRNUID>0\
        <STATUS><CODE>0<SEVERITY>INFO</STATUS><STMTRS><CURDEF>CAD<BANKACCTFROM><BANKID>1234<ACCTID>\
        1111111111111111<ACCTTYPE>CREDITLINE</BANKACCTFROM><BANKTRANLIST><DTSTART>\
        20241102200000.000[-4:EDT]<DTEND>20241120190000.000[-5:EST]
        <STMTTRN>\
            <TRNTYPE>DEBIT\
            <DTPOSTED>20241115120000.000\
            <TRNAMT>-0.5\
            <FITID>0000000000001\
            <NAME>PARKING PAY MACHINE\
        </STMTTRN>\
        <STMTTRN>\
            <TRNTYPE>DEBIT\
            <DTPOSTED>20241116120000.000\
            <TRNAMT>-7.88\
            <FITID>0000000000002\
            <NAME>SQ ICECREAM\
            <MEMO>Rewards earned: 0.04 ~ Category: Other\
        </STMTTRN>\
        <STMTTRN>\
            <TRNTYPE>DEBIT\
            <DTPOSTED>20241116120000.000\
            <TRNAMT>-7.35\
            <FITID>0000000000003\
            <NAME>PIZZA RESTAURANT\
            <MEMO>Rewards earned: 0.04 ~ Category: Restaurant\
        </STMTTRN>\
        <STMTTRN>\
            <TRNTYPE>DEBIT\
            <DTPOSTED>20241112120000.000\
            <TRNAMT>-8.91\
            <FITID>0000000000004\
            <NAME>City Mall\
            <MEMO>Rewards earned: 0.18 ~ Category: Entertainment\
        </STMTTRN>\
        </BANKTRANLIST><LEDGERBAL><BALAMT>-276.39<DTASOF>20241120170806.513[-5:EST]</LEDGERBAL>\
        <AVAILBAL><BALAMT>-11692.05<DTASOF>20241120170806.513[-5:EST]</AVAILBAL></STMTRS>\
        </STMTTRNRS></BANKMSGSRSV1></OFX>\
        ";

    #[test]
    fn test_parse() {
        let transactions = parse(&SAMPLE).unwrap();
        assert_eq!(
            transactions,
            vec![
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
                    amount: -0.5,
                    name: Some("PARKING PAY MACHINE".into()),
                    memo: None,
                },
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 11, 16).unwrap(),
                    amount: -7.88,
                    name: Some("SQ ICECREAM".into()),
                    memo: Some("Rewards earned: 0.04 ~ Category: Other".into()),
                },
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 11, 16).unwrap(),
                    amount: -7.35,
                    name: Some("PIZZA RESTAURANT".into()),
                    memo: Some("Rewards earned: 0.04 ~ Category: Restaurant".into()),
                },
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 11, 12).unwrap(),
                    amount: -8.91,
                    name: Some("City Mall".into()),
                    memo: Some("Rewards earned: 0.18 ~ Category: Entertainment".into()),
                }
            ]
        );
    }
}
