use std::fs;
use std::path::PathBuf;

use super::error::ImportError;
use anyhow::Result;
use chrono::{self, NaiveDateTime, ParseResult};
use chrono::{DateTime, NaiveDate};
use regex::{Captures, Regex};
use serde::{de, Deserialize, Deserializer};
use sgmlish::{self, SgmlFragment};

#[derive(Debug, Deserialize)]
struct Ofx {
    #[serde(rename = "STMTTRN")]
    transactions: Vec<OfxTransaction>,
}

fn parse_date(s: &str) -> ParseResult<NaiveDate> {
    let re = Regex::new(r"(\.\d+)?\[([\+-])(\d+):[a-zA-Z]+\]").unwrap();
    let s = re
        .replace(s, |caps: &Captures| format!("{}{:0>2}", &caps[2], &caps[3]))
        .to_string();

    DateTime::parse_from_str(&s, r"%Y%m%d%H%M%S%#z")
        .map(|dt| dt.date_naive())
        .or_else(|_| NaiveDateTime::parse_from_str(&s, r"%Y%m%d%H%M%S%.3f").map(|dt| dt.date()))
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
            parse_date(v).map_err(|_| E::custom(format!("Failed to parse datetime: {}", v)))
        }
    }

    deserializer.deserialize_str(YMDStringVisitor)
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum TransactionKind {
    DEBIT = 1,
    CREDIT = 2,
    OTHER = 3,
    ATM = 4,
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

fn parse(file_contents: &str) -> Result<Vec<OfxTransaction>, sgmlish::Error> {
    let xml = get_ofx_block(file_contents).unwrap();
    let sgml = sgmlish::Parser::builder()
        .uppercase_names()
        .expand_entities(|entity| match entity {
            "lt" => Some("<"),
            "gt" => Some(">"),
            "amp" => Some("&"),
            "nbsp" => Some(" "),
            _ => None,
        })
        .parse(xml)?;

    let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;

    let mut events = Vec::new();
    let mut include = false;

    // Search for the BANKTRANLIST tag
    for event in sgml.iter() {
        match event {
            sgmlish::SgmlEvent::OpenStartTag { name } => {
                if &name.to_uppercase() == "BANKTRANLIST" {
                    include = true;
                }
            }
            sgmlish::SgmlEvent::EndTag { name } => {
                if &name.to_uppercase() == "BANKTRANLIST" {
                    events.push(event.clone());
                    break;
                }
            }
            _ => (),
        }
        if include {
            events.push(event.clone());
        }
    }
    let sgml = SgmlFragment::from(events);
    let result = sgmlish::from_fragment::<Ofx>(sgml)?;
    Ok(result.transactions)
}

pub fn load_transactions(path: &PathBuf) -> Result<Vec<OfxTransaction>> {
    let content = fs::read_to_string(path)?;
    let ts = parse(&content).map_err(|err| ImportError::from(err))?;
    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse() {
        let transactions = parse(
            "\
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
            <FID>12345</FI><INTU.BID>12345</SONRS></SIGNONMSGSRSV1><BANKMSGSRSV1><STMTTRNRS>\
            <TRNUID>0<STATUS><CODE>0<SEVERITY>INFO</STATUS><STMTRS><CURDEF>CAD<BANKACCTFROM>\
            <BANKID>1234<ACCTID>1111111111111111<ACCTTYPE>CREDITLINE</BANKACCTFROM><BANKTRANLIST>\
            <DTSTART>20241102200000.000[-4:EDT]<DTEND>20241120190000.000[-5:EST]
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
            ",
        )
        .unwrap();
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

    #[test]
    fn test_parse_date() {
        assert_eq!(
            parse_date("20211217215753.211[-8:PST]").unwrap(),
            NaiveDate::from_ymd_opt(2021, 12, 17).unwrap()
        );
        assert_eq!(
            parse_date("20211130000000[-8:PST]").unwrap(),
            NaiveDate::from_ymd_opt(2021, 11, 30).unwrap()
        );
        assert_eq!(
            parse_date("20240731200000.000[-4:EDT]").unwrap(),
            NaiveDate::from_ymd_opt(2024, 07, 31).unwrap()
        );
        assert_eq!(
            parse_date("20241108120000.000").unwrap(),
            NaiveDate::from_ymd_opt(2024, 11, 08).unwrap()
        );
    }

    #[test]
    fn test_parse_credit_card() {
        let transactions = parse(
            "OFXHEADER:100\
            DATA:OFXSGML\
            VERSION:102\
            SECURITY:NONE\
            ENCODING:USASCII\
            CHARSET:1252\
            COMPRESSION:NONE\
            OLDFILEUID:NONE\
            NEWFILEUID:NONE\
            <OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO<MESSAGE>OK</STATUS>\
            <DTSERVER>20241226044534<LANGUAGE>ENG<DTPROFUP>20241226044534<DTACCTUP>20241226044534\
            <INTU.BID>00000</SONRS></SIGNONMSGSRSV1><CREDITCARDMSGSRSV1><CCSTMTTRNRS><TRNUID>\
            20241226044534<STATUS><CODE>0<SEVERITY>INFO<MESSAGE>OK</STATUS><CCSTMTRS><CURDEF>CAD\
            <CCACCTFROM><ACCTID>0000000000000000<ACCTTYPE>CREDITLINE</CCACCTFROM> \
            <BANKTRANLIST><DTSTART>20241218120000<DTEND>20241223120000 \
            <STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20241223120000.000[-5:EST]<TRNAMT>-6.10<FITID>\
            00000000000001<NAME>GAS STATION 123<MEMO>TOWN NAME;CC#0000********0000</STMTTRN>\
            <STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20241223120000.000[-5:EST]<TRNAMT>-44.46<FITID>\
            00000000000002<NAME>GAS STATION 123<MEMO>TOWN NAME;CC#0000********0000</STMTTRN>\
            <STMTTRN><TRNTYPE>CREDIT<DTPOSTED>20241218120000.000[-5:EST]<TRNAMT>152.98<FITID>\
            00000000000003<NAME>PAYMENT THANK YOU/PAIEMEN<MEMO>CC#0000********0000</STMTTRN>\
            </BANKTRANLIST><LEDGERBAL><BALAMT>-50.56<DTASOF>20241226044534</LEDGERBAL><AVAILBAL>\
            <BALAMT>9949.44<DTASOF>20241226044534</AVAILBAL></CCSTMTRS></CCSTMTTRNRS>\
            </CREDITCARDMSGSRSV1></OFX>",
        )
        .unwrap();
        assert_eq!(
            transactions,
            vec![
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 12, 23).unwrap(),
                    amount: -6.10,
                    name: Some("GAS STATION 123".into()),
                    memo: Some("TOWN NAME;CC#0000********0000".into()),
                },
                OfxTransaction {
                    transaction_kind: TransactionKind::DEBIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 12, 23).unwrap(),
                    amount: -44.46,
                    name: Some("GAS STATION 123".into()),
                    memo: Some("TOWN NAME;CC#0000********0000".into()),
                },
                OfxTransaction {
                    transaction_kind: TransactionKind::CREDIT,
                    date_posted: NaiveDate::from_ymd_opt(2024, 12, 18).unwrap(),
                    amount: 152.98,
                    name: Some("PAYMENT THANK YOU/PAIEMEN".into()),
                    memo: Some("CC#0000********0000".into()),
                }
            ]
        );
    }
}
