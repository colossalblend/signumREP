                                      
   
                                                                               
                                                                        
                                                             
   
            
                                                                                  
                                                                                           
                                                                                   
                                                                          
   
                     
           
                                                                                  
                                                                                                 
                                                                                 
                                                                                                               
                                                                                                
                                                                                                                        
                                                          
                                                                      
                                                                
       
   
                       
           
     
                                  
                                  
                               
                                                   
                                        
                                           
                                           
                               
                                          
                                   
                                    
     
       
                                                                        
                                                                                    
                                                                                   
                                                   

use rand::seq::SliceRandom;
use rand::thread_rng;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, SET_COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

const BUY: &str = "https://buy.apps.apple.com";
const AMP: &str = "https://amp-account.apps.apple.com";
const AUTH_GS: &str = "https://auth.apps.apple.com/auth/v1/gs";
const RUS_STORE_FRONT: &str = "143469-1,8";
const USA_STORE_FRONT: &str = "143441-1,8";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36";
                                                                        
const AMP_BEARER: &str = "Bearer eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjU1VDNUMjM4UTQifQ.eyJpc3MiOiJBTVBXZWJQbGF5IiwiaWF0IjoxNzgyODY0MjQyLCJleHAiOjE3OTg0MTYyNDIsInJvb3RfaHR0cHNfb3JpZ2luIjpbImFwcGxlLmNvbSIsIm16c3RhdGljLmNvbSJdfQ.NWIMDTslEElpGnFqe_in3257-gsnrSGONKs1fK6sPHkVyrOoBRqUoOtk25GtsT3urf3r-tvlPOGF6OLAB0_bzw";

                                                                              
               
                                                                              

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRegionOption {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedAddress {
    pub country_code: String,
    pub country_name: String,
    pub street: String,
    pub city: String,
    pub postal_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub county: Option<String>,
    pub phone_area: String,
    pub phone_number: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRegionStatus {
    pub country_code: String,
    pub country_name: String,
    pub needs_2fa: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRegionResult {
    pub ok: bool,
    pub needs_2fa: bool,
    pub country_code: String,
    pub country_name: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRegionInput {
    #[serde(default)]
    pub email: Option<String>,
    pub country: String,
    #[serde(default)]
    pub address: Option<GeneratedAddress>,
    #[serde(default)]
    pub two_factor_code: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub resend_code: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionQueryInput {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub two_factor_code: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub resend_code: Option<bool>,
}

                                                                              
                                                        
                                                         
                                                                              

struct AddrTpl {
    iso3: &'static str,
    name: &'static str,
    street: &'static str,
    city: &'static str,
    postal: &'static str,
    state: Option<&'static str>,
    county: Option<&'static str>,
    phone_area: &'static str,
    phone_number: &'static str,
}

                                                                     
const ADDRESSES: &[AddrTpl] = &[
                                    
    AddrTpl {
        iso3: "RUS",
        name: "Россия",
        street: "Tverskaya Street 7",
        city: "Moscow",
        postal: "125009",
        state: None,
        county: None,
        phone_area: "495",
        phone_number: "6290001",
    },
    AddrTpl {
        iso3: "RUS",
        name: "Россия",
        street: "Nevsky Prospect 28",
        city: "Saint Petersburg",
        postal: "191186",
        state: None,
        county: None,
        phone_area: "812",
        phone_number: "5710001",
    },
    AddrTpl {
        iso3: "BLR",
        name: "Беларусь",
        street: "Nezavisimosti Avenue 15",
        city: "Minsk",
        postal: "220030",
        state: None,
        county: None,
        phone_area: "17",
        phone_number: "2290001",
    },
    AddrTpl {
        iso3: "UKR",
        name: "Украина",
        street: "Khreshchatyk Street 22",
        city: "Kyiv",
        postal: "01001",
        state: None,
        county: None,
        phone_area: "44",
        phone_number: "2790001",
    },
                                           
    AddrTpl {
        iso3: "GBR",
        name: "Великобритания",
        street: "221B Baker Street",
        city: "London",
        postal: "NW1 6XE",
        state: None,
        county: Some("Greater London"),
        phone_area: "20",
        phone_number: "79460001",
    },
    AddrTpl {
        iso3: "JPN",
        name: "Япония",
        street: "1-1 Marunouchi",
        city: "Chiyoda-ku",
        postal: "100-0005",
        state: Some("Tokyo"),
        county: None,
        phone_area: "3",
        phone_number: "32110001",
    },
    AddrTpl {
        iso3: "KOR",
        name: "Южная Корея",
        street: "110 Sejong-daero",
        city: "Jung-gu",
        postal: "04524",
        state: Some("Seoul"),
        county: None,
        phone_area: "2",
        phone_number: "1200001",
    },
    AddrTpl {
        iso3: "CHN",
        name: "Китай",
        street: "1 Changan Avenue",
        city: "Dongcheng",
        postal: "100006",
        state: Some("Beijing"),
        county: None,
        phone_area: "10",
        phone_number: "65130001",
    },
    AddrTpl {
        iso3: "BRA",
        name: "Бразилия",
        street: "Avenida Paulista 1000",
        city: "Sao Paulo",
        postal: "01310-100",
        state: Some("SP"),
        county: None,
        phone_area: "11",
        phone_number: "32510001",
    },
    AddrTpl {
        iso3: "TUR",
        name: "Турция",
        street: "Istiklal Caddesi 120",
        city: "Istanbul",
        postal: "34430",
        state: None,
        county: Some("Beyoglu"),
        phone_area: "212",
        phone_number: "2440001",
    },
    AddrTpl {
        iso3: "ARE",
        name: "ОАЭ",
        street: "Sheikh Zayed Road 1",
        city: "Dubai",
        postal: "00000",
        // UAE forms reject addressOfficialStateProvince ("validStateProv"), so
        // no emirate is sent at all. Apple only needs city + postal code here.
        state: None,
        county: None,
        phone_area: "4",
        phone_number: "3010001",
    },
                                                    
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "350 5th Avenue",
        city: "New York",
        postal: "10118",
        state: Some("NY"),
        county: None,
        phone_area: "212",
        phone_number: "7363100",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "1 Apple Park Way",
        city: "Cupertino",
        postal: "95014",
        state: Some("CA"),
        county: None,
        phone_area: "408",
        phone_number: "9961010",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "410 Terry Avenue North",
        city: "Seattle",
        postal: "98109",
        state: Some("WA"),
        county: None,
        phone_area: "206",
        phone_number: "2661000",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "1600 Amphitheatre Parkway",
        city: "Mountain View",
        postal: "94043",
        state: Some("CA"),
        county: None,
        phone_area: "650",
        phone_number: "2530000",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "1 Microsoft Way",
        city: "Redmond",
        postal: "98052",
        state: Some("WA"),
        county: None,
        phone_area: "425",
        phone_number: "8828080",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "233 South Wacker Drive",
        city: "Chicago",
        postal: "60606",
        state: Some("IL"),
        county: None,
        phone_area: "312",
        phone_number: "8750066",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "200 Clarendon Street",
        city: "Boston",
        postal: "02116",
        state: Some("MA"),
        county: None,
        phone_area: "617",
        phone_number: "2666000",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "1201 South Figueroa Street",
        city: "Los Angeles",
        postal: "90015",
        state: Some("CA"),
        county: None,
        phone_area: "213",
        phone_number: "7427100",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "600 Congress Avenue",
        city: "Austin",
        postal: "78701",
        state: Some("TX"),
        county: None,
        phone_area: "512",
        phone_number: "4041900",
    },
    AddrTpl {
        iso3: "USA",
        name: "США",
        street: "701 Brickell Avenue",
        city: "Miami",
        postal: "33131",
        state: Some("FL"),
        county: None,
        phone_area: "305",
        phone_number: "5773000",
    },
                                           
    AddrTpl {
        iso3: "KAZ",
        name: "Казахстан",
        street: "Dostyk Avenue 38",
        city: "Almaty",
        postal: "050010",
        state: None,
        county: None,
        phone_area: "727",
        phone_number: "2588888",
    },
    AddrTpl {
        iso3: "KAZ",
        name: "Казахстан",
        street: "Kabanbay Batyr 53",
        city: "Astana",
        postal: "010000",
        state: None,
        county: None,
        phone_area: "717",
        phone_number: "2700000",
    },
    AddrTpl {
        iso3: "ARM",
        name: "Армения",
        street: "Northern Avenue 1",
        city: "Yerevan",
        postal: "0001",
        state: None,
        county: None,
        phone_area: "10",
        phone_number: "539000",
    },
    AddrTpl {
        iso3: "ARM",
        name: "Армения",
        street: "Mashtots Avenue 15",
        city: "Yerevan",
        postal: "0002",
        state: None,
        county: None,
        phone_area: "10",
        phone_number: "500500",
    },
    AddrTpl {
        iso3: "GEO",
        name: "Грузия",
        street: "Rustaveli Avenue 12",
        city: "Tbilisi",
        postal: "0108",
        state: None,
        county: None,
        phone_area: "32",
        phone_number: "2990000",
    },
    AddrTpl {
        iso3: "GEO",
        name: "Грузия",
        street: "Chavchavadze Avenue 37",
        city: "Tbilisi",
        postal: "0179",
        state: None,
        county: None,
        phone_area: "32",
        phone_number: "2250000",
    },
    AddrTpl {
        iso3: "AZE",
        name: "Азербайджан",
        street: "Nizami Street 203",
        city: "Baku",
        postal: "AZ1000",
        state: None,
        county: None,
        phone_area: "12",
        phone_number: "4930000",
    },
    AddrTpl {
        iso3: "AZE",
        name: "Азербайджан",
        street: "Neftchilar Avenue 26A",
        city: "Baku",
        postal: "AZ1000",
        state: None,
        county: None,
        phone_area: "12",
        phone_number: "4040000",
    },
    AddrTpl {
        iso3: "KGZ",
        name: "Киргизия",
        street: "Chuy Avenue 114",
        city: "Bishkek",
        postal: "720040",
        state: None,
        county: None,
        phone_area: "312",
        phone_number: "661000",
    },
    AddrTpl {
        iso3: "KGZ",
        name: "Киргизия",
        street: "Manas Avenue 40",
        city: "Bishkek",
        postal: "720001",
        state: None,
        county: None,
        phone_area: "312",
        phone_number: "620000",
    },
    AddrTpl {
        iso3: "UZB",
        name: "Узбекистан",
        street: "Amir Temur Avenue 1",
        city: "Tashkent",
        postal: "100000",
        state: None,
        county: None,
        phone_area: "71",
        phone_number: "2330000",
    },
    AddrTpl {
        iso3: "UZB",
        name: "Узбекистан",
        street: "Navoi Street 12",
        city: "Tashkent",
        postal: "100011",
        state: None,
        county: None,
        phone_area: "71",
        phone_number: "2440000",
    },
    AddrTpl {
        iso3: "MNG",
        name: "Монголия",
        street: "Peace Avenue 17",
        city: "Ulaanbaatar",
        postal: "14200",
        state: None,
        county: None,
        phone_area: "11",
        phone_number: "310000",
    },
    AddrTpl {
        iso3: "MDA",
        name: "Молдова",
        street: "Stefan cel Mare 1",
        city: "Chisinau",
        postal: "MD-2001",
        state: None,
        county: None,
        phone_area: "22",
        phone_number: "250000",
    },
                      
    AddrTpl {
        iso3: "PAN",
        name: "Панама",
        street: "Calle 50",
        city: "Panama City",
        postal: "0801",
        state: None,
        county: None,
        phone_area: "507",
        phone_number: "3000000",
    },
    AddrTpl {
        iso3: "PAN",
        name: "Панама",
        street: "Avenida Balboa 101",
        city: "Panama City",
        postal: "0801",
        state: None,
        county: None,
        phone_area: "507",
        phone_number: "3020000",
    },
    AddrTpl {
        iso3: "MEX",
        name: "Мексика",
        street: "Paseo de la Reforma 222",
        city: "Mexico City",
        postal: "06600",
        state: None,
        county: None,
        phone_area: "55",
        phone_number: "52000000",
    },
    AddrTpl {
        iso3: "MEX",
        name: "Мексика",
        street: "Avenida Insurgentes Sur 1602",
        city: "Mexico City",
        postal: "03940",
        state: None,
        county: None,
        phone_area: "55",
        phone_number: "56000000",
    },
    AddrTpl {
        iso3: "COL",
        name: "Колумбия",
        street: "Carrera 7 71-21",
        city: "Bogota",
        postal: "110231",
        state: None,
        county: None,
        phone_area: "1",
        phone_number: "6000000",
    },
    AddrTpl {
        iso3: "CHL",
        name: "Чили",
        street: "Avenida Apoquindo 3000",
        city: "Santiago",
        postal: "7550000",
        state: None,
        county: None,
        phone_area: "2",
        phone_number: "23300000",
    },
    AddrTpl {
        iso3: "PER",
        name: "Перу",
        street: "Avenida Javier Prado Este 4200",
        city: "Lima",
        postal: "15023",
        state: None,
        county: None,
        phone_area: "1",
        phone_number: "2110000",
    },
    AddrTpl {
        iso3: "ARG",
        name: "Аргентина",
        street: "Avenida 9 de Julio 1925",
        city: "Buenos Aires",
        postal: "C1073",
        state: None,
        county: None,
        phone_area: "11",
        phone_number: "43790000",
    },
    AddrTpl {
        iso3: "CAN",
        name: "Канада",
        street: "100 Queen Street West",
        city: "Toronto",
        postal: "M5H 2N2",
        state: Some("ON"),
        county: None,
        phone_area: "416",
        phone_number: "3922486",
    },
    AddrTpl {
        iso3: "CAN",
        name: "Канада",
        street: "800 Burrard Street",
        city: "Vancouver",
        postal: "V6Z 2V8",
        state: Some("BC"),
        county: None,
        phone_area: "604",
        phone_number: "8737000",
    },
                   
    AddrTpl {
        iso3: "PHL",
        name: "Филиппины",
        street: "30th Street",
        city: "Taguig",
        postal: "1634",
        state: None,
        county: Some("Metro Manila"),
        phone_area: "02",
        phone_number: "88881000",
    },
    AddrTpl {
        iso3: "PHL",
        name: "Филиппины",
        street: "Ayala Avenue",
        city: "Makati",
        postal: "1226",
        state: None,
        county: Some("Metro Manila"),
        phone_area: "02",
        phone_number: "88881001",
    },
    AddrTpl {
        iso3: "PHL",
        name: "Филиппины",
        street: "Ortigas Avenue",
        city: "Pasig",
        postal: "1605",
        state: None,
        county: Some("Metro Manila"),
        phone_area: "02",
        phone_number: "86880000",
    },
    AddrTpl {
        iso3: "IND",
        name: "Индия",
        street: "MG Road 1",
        city: "Bengaluru",
        postal: "560001",
        state: Some("KA"),
        county: None,
        phone_area: "80",
        phone_number: "25580000",
    },
    AddrTpl {
        iso3: "IDN",
        name: "Индонезия",
        street: "Jalan Jenderal Sudirman 52-53",
        city: "Jakarta",
        postal: "12190",
        state: None,
        county: None,
        phone_area: "21",
        phone_number: "25540000",
    },
    AddrTpl {
        iso3: "THA",
        name: "Таиланд",
        street: "Ratchadamri Road 2",
        city: "Bangkok",
        postal: "10330",
        state: None,
        county: None,
        phone_area: "2",
        phone_number: "2541234",
    },
    AddrTpl {
        iso3: "VNM",
        name: "Вьетнам",
        street: "Le Duan 2",
        city: "Ho Chi Minh City",
        postal: "700000",
        state: None,
        county: None,
        phone_area: "28",
        phone_number: "38220000",
    },
    AddrTpl {
        iso3: "MYS",
        name: "Малайзия",
        street: "Jalan Ampang 1",
        city: "Kuala Lumpur",
        postal: "50450",
        state: None,
        county: None,
        phone_area: "3",
        phone_number: "21610000",
    },
    AddrTpl {
        iso3: "SGP",
        name: "Сингапур",
        street: "Orchard Road 391",
        city: "Singapore",
        postal: "238873",
        state: None,
        county: None,
        phone_area: "65",
        phone_number: "67331118",
    },
    AddrTpl {
        iso3: "HKG",
        name: "Гонконг",
        street: "Queensway 1",
        city: "Hong Kong",
        postal: "00000",
        state: None,
        county: None,
        phone_area: "852",
        phone_number: "25228888",
    },
    AddrTpl {
        iso3: "TWN",
        name: "Тайвань",
        street: "Zhongxiao East Road 1",
        city: "Taipei",
        postal: "100",
        state: None,
        county: None,
        phone_area: "2",
        phone_number: "23412345",
    },
    AddrTpl {
        iso3: "AUS",
        name: "Австралия",
        street: "1 Macquarie Place",
        city: "Sydney",
        postal: "2000",
        state: Some("NSW"),
        county: None,
        phone_area: "2",
        phone_number: "92507000",
    },
    AddrTpl {
        iso3: "NZL",
        name: "Новая Зеландия",
        street: "Queen Street 188",
        city: "Auckland",
        postal: "1010",
        state: None,
        county: None,
        phone_area: "9",
        phone_number: "3077000",
    },
    AddrTpl {
        iso3: "ZAF",
        name: "ЮАР",
        street: "5th Street 90",
        city: "Sandton",
        postal: "2196",
        state: None,
        county: None,
        phone_area: "11",
        phone_number: "2173000",
    },
    AddrTpl {
        iso3: "EGY",
        name: "Египет",
        street: "Tahrir Square 1",
        city: "Cairo",
        postal: "11511",
        state: None,
        county: None,
        phone_area: "2",
        phone_number: "25780000",
    },
    AddrTpl {
        iso3: "NGA",
        name: "Нигерия",
        street: "Adeola Odeku Street 1",
        city: "Lagos",
        postal: "101241",
        state: None,
        county: None,
        phone_area: "1",
        phone_number: "2700000",
    },
    AddrTpl {
        iso3: "KEN",
        name: "Кения",
        street: "Kenyatta Avenue 1",
        city: "Nairobi",
        postal: "00100",
        state: None,
        county: None,
        phone_area: "20",
        phone_number: "2221000",
    },
    AddrTpl {
        iso3: "SRB",
        name: "Сербия",
        street: "Knez Mihailova 1",
        city: "Belgrade",
        postal: "11000",
        state: None,
        county: None,
        phone_area: "11",
        phone_number: "2020000",
    },
    AddrTpl {
        iso3: "ALB",
        name: "Албания",
        street: "Bulevardi Deshmoret e Kombit 1",
        city: "Tirana",
        postal: "1001",
        state: None,
        county: None,
        phone_area: "4",
        phone_number: "2230000",
    },
];

pub fn list_store_regions() -> Vec<StoreRegionOption> {
    let mut out = Vec::new();
    for row in ADDRESSES {
        if is_card_only_region(row.iso3) {
            continue;
        }
        if out.iter().any(|o: &StoreRegionOption| o.code == row.iso3) {
            continue;
        }
        out.push(StoreRegionOption {
            code: row.iso3.to_string(),
            name: row.name.to_string(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

const ADDRESS_STREETS: [(&str, &str); 10] = [
    ("Central Avenue", "14"),
    ("Market Street", "27"),
    ("Park Lane", "38"),
    ("Riverside Drive", "51"),
    ("Liberty Square", "8"),
    ("Garden Road", "64"),
    ("Station Street", "19"),
    ("Oak Avenue", "72"),
    ("Harbor Road", "33"),
    ("Sunrise Boulevard", "46"),
];

                                                                               
                                                                                   
                                                                 
fn address_localities(iso3: &str) -> &'static [&'static str] {
    match iso3 {
        "ALB" => &[
            "Tirana",
            "Durres",
            "Vlore",
            "Shkoder",
            "Elbasan",
            "Korce",
            "Fier",
            "Berat",
            "Gjirokaster",
            "Kukes",
        ],
        "ARE" => &[
            "Dubai",
            "Abu Dhabi",
            "Sharjah",
            "Ajman",
            "Al Ain",
            "Ras Al Khaimah",
            "Fujairah",
            "Umm Al Quwain",
            "Khor Fakkan",
            "Kalba",
        ],
        "ARG" => &[
            "Buenos Aires",
            "Cordoba",
            "Rosario",
            "Mendoza",
            "La Plata",
            "Mar del Plata",
            "Salta",
            "Tucuman",
            "Santa Fe",
            "Neuquen",
        ],
        "ARM" => &[
            "Yerevan",
            "Gyumri",
            "Vanadzor",
            "Vagharshapat",
            "Hrazdan",
            "Kapan",
            "Armavir",
            "Artashat",
            "Ijevan",
            "Goris",
        ],
        "AUS" => &[
            "Sydney",
            "Melbourne",
            "Brisbane",
            "Perth",
            "Adelaide",
            "Canberra",
            "Gold Coast",
            "Newcastle",
            "Geelong",
            "Hobart",
        ],
        "AZE" => &[
            "Baku",
            "Ganja",
            "Sumqayit",
            "Mingachevir",
            "Lankaran",
            "Shirvan",
            "Nakhchivan",
            "Shaki",
            "Yevlakh",
            "Khirdalan",
        ],
        "BLR" => &[
            "Minsk",
            "Gomel",
            "Vitebsk",
            "Grodno",
            "Brest",
            "Mogilev",
            "Bobruisk",
            "Baranavichy",
            "Borisov",
            "Pinsk",
        ],
        "BRA" => &[
            "Sao Paulo",
            "Rio de Janeiro",
            "Brasilia",
            "Salvador",
            "Fortaleza",
            "Belo Horizonte",
            "Manaus",
            "Curitiba",
            "Recife",
            "Porto Alegre",
        ],
        "CAN" => &[
            "Toronto",
            "Montreal",
            "Vancouver",
            "Calgary",
            "Edmonton",
            "Ottawa",
            "Winnipeg",
            "Quebec City",
            "Hamilton",
            "Halifax",
        ],
        "CHL" => &[
            "Santiago",
            "Valparaiso",
            "Concepcion",
            "La Serena",
            "Antofagasta",
            "Vina del Mar",
            "Temuco",
            "Rancagua",
            "Talca",
            "Puerto Montt",
        ],
        "CHN" => &[
            "Dongcheng",
            "Shanghai",
            "Guangzhou",
            "Shenzhen",
            "Chengdu",
            "Hangzhou",
            "Wuhan",
            "Nanjing",
            "Chongqing",
            "Tianjin",
        ],
        "COL" => &[
            "Bogota",
            "Medellin",
            "Cali",
            "Barranquilla",
            "Cartagena",
            "Bucaramanga",
            "Pereira",
            "Manizales",
            "Cucuta",
            "Santa Marta",
        ],
        "EGY" => &[
            "Cairo",
            "Alexandria",
            "Giza",
            "Luxor",
            "Aswan",
            "Port Said",
            "Suez",
            "Mansoura",
            "Tanta",
            "Hurghada",
        ],
        "GBR" => &[
            "London",
            "Birmingham",
            "Manchester",
            "Leeds",
            "Glasgow",
            "Liverpool",
            "Bristol",
            "Edinburgh",
            "Sheffield",
            "Belfast",
        ],
        "GEO" => &[
            "Tbilisi",
            "Batumi",
            "Kutaisi",
            "Rustavi",
            "Gori",
            "Zugdidi",
            "Poti",
            "Telavi",
            "Akhaltsikhe",
            "Mtskheta",
        ],
        "HKG" => &[
            "Hong Kong",
            "Kowloon",
            "Tsuen Wan",
            "Sha Tin",
            "Tuen Mun",
            "Yuen Long",
            "Tai Po",
            "Kwun Tong",
            "Sai Kung",
            "Tseung Kwan O",
        ],
        "IDN" => &[
            "Jakarta",
            "Surabaya",
            "Bandung",
            "Medan",
            "Semarang",
            "Makassar",
            "Palembang",
            "Yogyakarta",
            "Denpasar",
            "Balikpapan",
        ],
        "IND" => &[
            "Bengaluru",
            "Mumbai",
            "Delhi",
            "Hyderabad",
            "Chennai",
            "Kolkata",
            "Pune",
            "Ahmedabad",
            "Jaipur",
            "Kochi",
        ],
        "JPN" => &[
            "Chiyoda-ku",
            "Osaka",
            "Yokohama",
            "Nagoya",
            "Sapporo",
            "Fukuoka",
            "Kobe",
            "Kyoto",
            "Sendai",
            "Hiroshima",
        ],
        "KAZ" => &[
            "Almaty",
            "Astana",
            "Shymkent",
            "Karaganda",
            "Aktobe",
            "Taraz",
            "Pavlodar",
            "Oskemen",
            "Semey",
            "Kyzylorda",
        ],
        "KEN" => &[
            "Nairobi", "Mombasa", "Kisumu", "Nakuru", "Eldoret", "Thika", "Malindi", "Machakos",
            "Nyeri", "Kitale",
        ],
        "KGZ" => &[
            "Bishkek",
            "Osh",
            "Jalal-Abad",
            "Karakol",
            "Tokmok",
            "Balykchy",
            "Naryn",
            "Talas",
            "Kara-Balta",
            "Batken",
        ],
        "KOR" => &[
            "Jung-gu", "Busan", "Incheon", "Daegu", "Daejeon", "Gwangju", "Suwon", "Ulsan",
            "Changwon", "Goyang",
        ],
        "MDA" => &[
            "Chisinau", "Balti", "Cahul", "Ungheni", "Orhei", "Soroca", "Comrat", "Edinet",
            "Hincesti", "Straseni",
        ],
        "MEX" => &[
            "Mexico City",
            "Guadalajara",
            "Monterrey",
            "Puebla",
            "Tijuana",
            "Merida",
            "Queretaro",
            "Leon",
            "Cancun",
            "Toluca",
        ],
        "MNG" => &[
            "Ulaanbaatar",
            "Erdenet",
            "Darkhan",
            "Khovd",
            "Choibalsan",
            "Murun",
            "Ulgii",
            "Sainshand",
            "Zuunmod",
            "Arvaikheer",
        ],
        "MYS" => &[
            "Kuala Lumpur",
            "George Town",
            "Johor Bahru",
            "Ipoh",
            "Shah Alam",
            "Kota Kinabalu",
            "Kuching",
            "Malacca City",
            "Alor Setar",
            "Kuala Terengganu",
        ],
        "NGA" => &[
            "Lagos",
            "Abuja",
            "Kano",
            "Ibadan",
            "Benin City",
            "Port Harcourt",
            "Kaduna",
            "Enugu",
            "Jos",
            "Ilorin",
        ],
        "NZL" => &[
            "Auckland",
            "Wellington",
            "Christchurch",
            "Hamilton",
            "Tauranga",
            "Dunedin",
            "Palmerston North",
            "Napier",
            "Nelson",
            "Rotorua",
        ],
        "PAN" => &[
            "Panama City",
            "Colon",
            "David",
            "Santiago",
            "Chitre",
            "La Chorrera",
            "Penonome",
            "Aguadulce",
            "Las Tablas",
            "Bocas del Toro",
        ],
        "PER" => &[
            "Lima", "Arequipa", "Trujillo", "Chiclayo", "Cusco", "Piura", "Iquitos", "Huancayo",
            "Tacna", "Chimbote",
        ],
        "PHL" => &[
            "Taguig",
            "Makati",
            "Pasig",
            "Quezon City",
            "Manila",
            "Cebu City",
            "Davao City",
            "Baguio",
            "Iloilo City",
            "Cagayan de Oro",
        ],
        "RUS" => &[
            "Moscow",
            "Saint Petersburg",
            "Kazan",
            "Yekaterinburg",
            "Novosibirsk",
            "Nizhny Novgorod",
            "Samara",
            "Rostov-on-Don",
            "Krasnodar",
            "Vladivostok",
        ],
        "SGP" => &[
            "Orchard",
            "Marina Bay",
            "Tampines",
            "Jurong East",
            "Woodlands",
            "Toa Payoh",
            "Bukit Timah",
            "Novena",
            "Punggol",
            "Bedok",
        ],
        "SRB" => &[
            "Belgrade",
            "Novi Sad",
            "Nis",
            "Kragujevac",
            "Subotica",
            "Pancevo",
            "Cacak",
            "Kraljevo",
            "Zrenjanin",
            "Smederevo",
        ],
        "THA" => &[
            "Bangkok",
            "Chiang Mai",
            "Pattaya",
            "Phuket",
            "Nakhon Ratchasima",
            "Khon Kaen",
            "Hat Yai",
            "Udon Thani",
            "Surat Thani",
            "Hua Hin",
        ],
        "TUR" => &[
            "Istanbul",
            "Ankara",
            "Izmir",
            "Bursa",
            "Antalya",
            "Adana",
            "Gaziantep",
            "Konya",
            "Kayseri",
            "Mersin",
        ],
        "TWN" => &[
            "Taipei",
            "Kaohsiung",
            "Taichung",
            "Tainan",
            "Hsinchu",
            "Keelung",
            "Taoyuan",
            "Chiayi",
            "Hualien",
            "Pingtung",
        ],
        "UKR" => &[
            "Kyiv",
            "Lviv",
            "Odesa",
            "Kharkiv",
            "Dnipro",
            "Zaporizhzhia",
            "Vinnytsia",
            "Ivano-Frankivsk",
            "Chernihiv",
            "Poltava",
        ],
        "USA" => &[
            "New York",
            "Cupertino",
            "Seattle",
            "Mountain View",
            "Redmond",
            "Chicago",
            "Boston",
            "Los Angeles",
            "Austin",
            "Miami",
        ],
        "UZB" => &[
            "Tashkent",
            "Samarkand",
            "Bukhara",
            "Namangan",
            "Andijan",
            "Fergana",
            "Nukus",
            "Qarshi",
            "Kokand",
            "Urgench",
        ],
        "VNM" => &[
            "Ho Chi Minh City",
            "Hanoi",
            "Da Nang",
            "Hai Phong",
            "Can Tho",
            "Hue",
            "Nha Trang",
            "Da Lat",
            "Vinh",
            "Bien Hoa",
        ],
        "ZAF" => &[
            "Johannesburg",
            "Cape Town",
            "Durban",
            "Pretoria",
            "Gqeberha",
            "Bloemfontein",
            "East London",
            "Polokwane",
            "Mbombela",
            "Kimberley",
        ],
        _ => &[],
    }
}

fn vary_last_digit(value: &str, offset: u8) -> String {
    if offset == 0 {
        return value.to_string();
    }
    let mut chars: Vec<char> = value.chars().collect();
    if let Some(index) = chars.iter().rposition(|ch| ch.is_ascii_digit()) {
        let digit = chars[index].to_digit(10).unwrap_or_default() as u8;
        chars[index] = char::from(b'0' + (digit + offset) % 10);
    }
    chars.into_iter().collect()
}

pub fn list_region_addresses(iso3: &str) -> Result<Vec<GeneratedAddress>, String> {
    let code = iso3.trim().to_ascii_uppercase();
    if code.is_empty() {
        return Err("Выберите страну.".into());
    }
    if is_card_only_region(&code) {
        return Err("Для этой страны Apple просит банковскую карту. Выберите другую.".into());
    }
    let mut addresses: Vec<_> = ADDRESSES
        .iter()
        .filter(|row| row.iso3 == code)
        .map(tpl_to_address)
        .collect();
    if addresses.is_empty() {
        return Err("Эту страну пока нельзя выбрать в Signum.".into());
    }

                                                                               
                                                                                  
                                                                                  
                                                                       
                                           
    if addresses.len() < 10 {
        let seed = addresses[0].clone();
        let localities = address_localities(&code);
        addresses.truncate(1);
        for index in 1..10 {
            let (street, house) = ADDRESS_STREETS[index];
            let mut extra = seed.clone();
            extra.city = localities
                .get(index)
                .copied()
                .unwrap_or(seed.city.as_str())
                .to_string();
            extra.street = format!("{street} {house}");
                                                                               
                                                                                
            extra.state = None;
            extra.county = None;
            extra.postal_code = vary_last_digit(&seed.postal_code, index as u8);
            extra.phone_number = vary_last_digit(&seed.phone_number, index as u8);
            addresses.push(extra);
        }
    }
    addresses.truncate(10);
    Ok(addresses)
}

pub fn country_name(iso3: &str) -> String {
    let code = iso3.trim().to_ascii_uppercase();
    ADDRESSES
        .iter()
        .find(|r| r.iso3 == code)
        .map(|r| r.name.to_string())
        .or_else(|| known_name(&code).map(|s| s.to_string()))
        .unwrap_or(code)
}

fn known_name(iso3: &str) -> Option<&'static str> {
    Some(match iso3 {
        "RUS" => "Россия",
        "UKR" => "Украина",
        "BLR" => "Беларусь",
        "GBR" => "Великобритания",
        "DEU" => "Германия",
        "FRA" => "Франция",
        "ITA" => "Италия",
        "ESP" => "Испания",
        "POL" => "Польша",
        "NLD" => "Нидерланды",
        "JPN" => "Япония",
        "KOR" => "Корея",
        "CHN" => "Китай",
        "BRA" => "Бразилия",
        "TUR" => "Турция",
        "ARE" => "ОАЭ",
        _ => return None,
    })
}

pub fn generate_address(iso3: &str) -> Result<GeneratedAddress, String> {
    let pool = list_region_addresses(iso3)?;
    let mut rng = thread_rng();
    let address = pool
        .choose(&mut rng)
        .cloned()
        .unwrap_or_else(|| pool[0].clone());
    Ok(address)
}

fn tpl_to_address(row: &AddrTpl) -> GeneratedAddress {
    GeneratedAddress {
        country_code: row.iso3.to_string(),
        country_name: row.name.to_string(),
        street: row.street.to_string(),
        city: row.city.to_string(),
        postal_code: row.postal.to_string(),
        state: row.state.map(|s| s.to_string()),
        county: row.county.map(|s| s.to_string()),
        phone_area: row.phone_area.to_string(),
        phone_number: row.phone_number.to_string(),
    }
}

fn is_card_only_region(iso3: &str) -> bool {
    matches!(
        iso3,
        "AUT"
            | "BEL"
            | "BGR"
            | "HRV"
            | "CYP"
            | "CZE"
            | "DNK"
            | "EST"
            | "FIN"
            | "FRA"
            | "DEU"
            | "GRC"
            | "HUN"
            | "IRL"
            | "ITA"
            | "LVA"
            | "LTU"
            | "LUX"
            | "MLT"
            | "NLD"
            | "POL"
            | "PRT"
            | "ROU"
            | "SVK"
            | "SVN"
            | "ESP"
            | "SWE"
            | "GBR"
            | "NOR"
            | "ISL"
            | "CHE"
            | "LIE"
    )
}

fn store_front_from_site(site: Option<&str>) -> Option<String> {
    match site.unwrap_or("").trim().to_ascii_uppercase().as_str() {
        "USA" | "US" => Some(USA_STORE_FRONT.to_string()),
        "RUS" | "RU" => Some(RUS_STORE_FRONT.to_string()),
        _ => None,
    }
}

fn latin_name(raw: &str, fallback: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return fallback.to_string();
    }
    if t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || c == '-' || c == '\'')
    {
        return t.chars().take(32).collect();
    }
    fallback.to_string()
}

                                                                              
              
                                                                              

#[derive(Clone, Default)]
struct CookieJar {
    map: HashMap<String, String>,
}

impl CookieJar {
    fn from_map(map: HashMap<String, String>) -> Self {
        Self { map }
    }

    fn header_value(&self) -> Option<String> {
        self.header_value_for(false)
    }

    fn commerce_header_value(&self) -> Option<String> {
        self.header_value_for(true)
    }

    fn header_value_for(&self, commerce_only: bool) -> Option<String> {
        if self.map.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = self
            .map
            .iter()
            .filter(|(name, _)| {
                !commerce_only
                    || matches!(
                        name.to_ascii_lowercase().as_str(),
                                                                          
                                                                       
                                                                        
                        "acn01"
                            | "caw"
                            | "caw-at"
                            | "commerce-authorization-token"
                            | "commerce-authorization-token-lite"
                            | "commerce-ds-session-id"
                            | "commerce-on-web-token"
                            | "dslang"
                            | "geo"
                            | "itspod"
                            | "myacinfo"
                            | "site"
                    )
            })
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        if parts.is_empty() {
            return None;
        }
        parts.sort();
        Some(parts.join("; "))
    }

    fn store_from_response(&mut self, headers: &HeaderMap) {
        for value in headers.get_all(SET_COOKIE) {
            let raw = header_as_str(value);
                                                                               
                                                                                 
            for part in split_set_cookie_header(&raw) {
                let first = part.split(';').next().unwrap_or("").trim();
                if let Some((name, val)) = first.split_once('=') {
                    let name = name.trim();
                    if name.is_empty() {
                        continue;
                    }
                                                                          
                                                                           
                    if !val.trim().is_empty() {
                        self.map.insert(name.to_string(), val.trim().to_string());
                    }
                }
            }
        }
    }

    fn apply_to_headers(&self, headers: &mut HeaderMap, commerce_only: bool) {
        let Some(raw) = self.header_value_for(commerce_only) else {
            return;
        };
        if let Ok(value) = HeaderValue::from_str(&raw) {
            headers.insert(COOKIE, value);
            return;
        }
                                                                           
        let mut kept = Vec::new();
        for part in raw.split("; ") {
            if HeaderValue::from_str(part).is_ok() {
                kept.push(part.to_string());
            }
        }
        if let Ok(value) = HeaderValue::from_str(&kept.join("; ")) {
            headers.insert(COOKIE, value);
        }
    }
}

fn header_as_str(value: &HeaderValue) -> String {
    match value.to_str() {
        Ok(text) => text.to_string(),
        Err(_) => String::from_utf8_lossy(value.as_bytes()).into_owned(),
    }
}

                                                                       
fn split_set_cookie_header(raw: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b',' {
            let rest = raw[i + 1..].trim_start();
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim();
                if !name.is_empty()
                    && name.is_ascii()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && !name.eq_ignore_ascii_case("expires")
                    && !name.eq_ignore_ascii_case("max-age")
                    && !name.eq_ignore_ascii_case("path")
                    && !name.eq_ignore_ascii_case("domain")
                    && !name.eq_ignore_ascii_case("secure")
                    && !name.eq_ignore_ascii_case("httponly")
                    && !name.eq_ignore_ascii_case("samesite")
                {
                    out.push(raw[start..i].trim());
                    start = i + 1;
                }
            }
        }
        i += 1;
    }
    let last = raw[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    if out.is_empty() {
        out.push(raw);
    }
    out
}

struct CommerceClient {
    client: reqwest::Client,
    cookies: CookieJar,
    auth: crate::apple_history::AccountAuthSession,
    store_front: Option<String>,
    pldfltcid: Option<String>,
    pltvcid: Option<String>,
}

impl CommerceClient {
    fn new(auth: crate::apple_history::AccountAuthSession) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(false)
            .user_agent(UA)
            .http1_only()
            .use_native_tls()
            .build()
            .map_err(|_| "Не удалось подключиться к Apple. Проверьте интернет.".to_string())?;
        let cookies = CookieJar::from_map(auth.cookies.clone());
        let store_front = store_front_from_site(cookies.map.get("site").map(String::as_str))
            .or_else(|| Some(RUS_STORE_FRONT.to_string()));
        Ok(Self {
            client,
            cookies,
            auth,
            store_front,
            pldfltcid: None,
            pltvcid: None,
        })
    }

    fn set_store_front_country(&mut self, country: &str) {
        self.store_front =
            store_front_from_site(Some(country)).or_else(|| Some(RUS_STORE_FRONT.to_string()));
    }

    fn account_auth_session(&self) -> crate::apple_history::AccountAuthSession {
        let mut auth = self.auth.clone();
        auth.cookies = self.cookies.map.clone();
        auth
    }

    fn apply_auth_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in [
            ("scnt", self.auth.scnt.as_deref()),
            ("X-Apple-ID-Session-Id", self.auth.session_id.as_deref()),
            ("X-Apple-Session-Token", self.auth.session_token.as_deref()),
            (
                "X-Apple-Repair-Session-Token",
                self.auth.repair_session_token.as_deref(),
            ),
            (
                "X-Apple-Auth-Attributes",
                self.auth.auth_attributes.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                if let Ok(value) = HeaderValue::from_str(value) {
                    headers.insert(name, value);
                }
            }
        }
    }

    fn ingest_auth_headers(&mut self, headers: &HeaderMap) {
        for (name, target) in [
            ("scnt", &mut self.auth.scnt),
            ("x-apple-id-session-id", &mut self.auth.session_id),
            ("x-apple-session-token", &mut self.auth.session_token),
            (
                "x-apple-repair-session-token",
                &mut self.auth.repair_session_token,
            ),
            ("x-apple-auth-attributes", &mut self.auth.auth_attributes),
        ] {
            if let Some(value) = headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
            {
                *target = Some(value.to_string());
            }
        }
    }

    fn ingest_gs_body(&mut self, text: &str) {
        let Ok(value) = serde_json::from_str::<JsonValue>(text) else {
            return;
        };
        if let Some(token) = value.get("pldfltcid").and_then(JsonValue::as_str) {
            self.pldfltcid = Some(token.to_string());
        }
        if let Some(token) = value.get("pltvcid").and_then(JsonValue::as_str) {
            self.pltvcid = Some(token.to_string());
        }
        if let Some(token) = value.get("itspod").and_then(JsonValue::as_str) {
            self.cookies.map.insert("itspod".into(), token.into());
        }
    }

    async fn request(
        &mut self,
        method: &str,
        url: &str,
        payload: Option<&JsonValue>,
    ) -> Result<(u16, String), String> {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(UA));
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(
            "Accept",
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        h.insert(
            "Accept-Language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        let account_request = url.starts_with("https://account.apple.com/")
            || url.starts_with("https://appleid.apple.com/");
        let gs_request = url.starts_with("https://auth.apps.apple.com/");
        if account_request {
            h.insert(
                "Referer",
                HeaderValue::from_static("https://account.apple.com/"),
            );
            if let Some(scnt) = self.auth.scnt.as_deref() {
                if let Ok(value) = HeaderValue::from_str(scnt) {
                    h.insert("scnt", value);
                }
            }
        } else {
                                                                              
                                                                                   
            h.insert(
                "Referer",
                HeaderValue::from_static("https://apps.apple.com/"),
            );
            h.insert("Origin", HeaderValue::from_static("https://apps.apple.com"));
            if gs_request || url.starts_with(BUY) {
                if let Ok(value) = HeaderValue::from_str(AMP_BEARER) {
                    h.insert("Authorization", value);
                }
            }
            if url.starts_with(BUY) {
                if let Some(front) = self.store_front.as_deref() {
                    if let Ok(value) = HeaderValue::from_str(front) {
                        h.insert("X-Apple-Store-Front", value);
                    }
                }
            }
        }
        let commerce = gs_request || url.starts_with(BUY) || url.starts_with(AMP);
        self.cookies.apply_to_headers(&mut h, commerce);
        let builder = match method {
            "POST" => self.client.post(url),
            _ => self.client.get(url),
        };
        let builder = builder.headers(h);
        let builder = if let Some(p) = payload {
            builder.body(p.to_string())
        } else {
            builder
        };
        let resp = builder.send().await.map_err(|e| {
            crate::diag_log::http_err(method, url, &e.to_string());
            "Нет связи с Apple. Проверьте интернет.".to_string()
        })?;
        let status = resp.status().as_u16();
        self.cookies.store_from_response(resp.headers());
        self.ingest_auth_headers(resp.headers());
        if let Some(front) = resp
            .headers()
            .get("x-set-apple-store-front")
            .and_then(|value| value.to_str().ok())
        {
            self.store_front = Some(front.to_string());
        }
        if let Some(loc) = resp
            .headers()
            .get("location")
            .or_else(|| resp.headers().get("Location"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
        {
            if (300..400).contains(&status) && !loc.is_empty() {
                let next = resolve_url(url, &loc);
                return Box::pin(self.request("GET", &next, None)).await;
            }
        }
        let text = resp.text().await.unwrap_or_default();
        crate::diag_log::http(method, url, status, "", text.as_bytes());
        if url == AUTH_GS || url.ends_with("/account/web/infoRefresh") {
            self.ingest_gs_body(&text);
        }
        Ok((status, text))
    }

    async fn get(&mut self, url: &str) -> Result<(u16, String), String> {
        self.request("GET", url, None).await
    }

    async fn post(&mut self, url: &str, body: &JsonValue) -> Result<(u16, String), String> {
        self.request("POST", url, Some(body)).await
    }

    async fn refresh_account_store_cookies(&mut self) {
        self.prime_account_page().await;
        let _ = self.get("https://account.apple.com/bootstrap/portal").await;
        let _ = self.get("https://appleid.apple.com/account/manage").await;
        let _ = self.get(&format!("{BUY}/account/web/infoRefresh")).await;
        let _ = self.get(&format!("{BUY}/account/web/info")).await;
    }

    async fn prime_account_page(&mut self) {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(
            "Accept",
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            "Accept-Language",
            HeaderValue::from_static("ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7"),
        );
        headers.insert(
            "Referer",
            HeaderValue::from_static("https://account.apple.com/"),
        );
        if let Some(cookies) = self.cookies.header_value() {
            if let Ok(value) = HeaderValue::from_str(&cookies) {
                headers.insert(COOKIE, value);
            }
        }
        let Ok(response) = self
            .client
            .get("https://account.apple.com/account/manage/section/information")
            .headers(headers)
            .send()
            .await
        else {
            return;
        };
        self.cookies.store_from_response(response.headers());
        self.ingest_auth_headers(response.headers());
    }

    async fn establish_store_session(&mut self) -> Result<(), String> {
        let mut last_stage = "account token";
        let mut last_status = 0u16;
        for attempt in 0..2 {
            self.prime_account_page().await;
            let (token_status, _) = self
                .get("https://appleid.apple.com/account/manage/gs/ws/token")
                .await?;
            last_stage = "account token";
            last_status = token_status;
            if (200..300).contains(&token_status) {
                                                                          
                                                                           
                                                                            
                let gs_body = json!({ "webAuthorizationFlowContext": "apps" });
                let (gs_status, _) = self.post(AUTH_GS, &gs_body).await?;
                last_stage = "GS";
                last_status = gs_status;
                if (200..300).contains(&gs_status) {
                                                                             
                                                                             
                    let (refresh_status, _) =
                        self.get(&format!("{BUY}/account/web/infoRefresh")).await?;
                    let (buy_status, buy_body) =
                        self.get(&format!("{BUY}/account/web/info")).await?;
                    if (200..300).contains(&buy_status) && !looks_like_auth_dialog(&buy_body) {
                        return Ok(());
                    }
                    if (200..300).contains(&refresh_status) {
                        last_stage = "buy info";
                        last_status = buy_status;
                    } else {
                        last_stage = "buy infoRefresh";
                        last_status = refresh_status;
                    }
                }
            }
            if attempt == 0 {
                self.refresh_account_store_cookies().await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        Err(format!(
            "Apple отклонил сессию магазина на этапе {last_stage} (HTTP {last_status}). Подтвердите вход ещё раз."
        ))
    }

    async fn current_country(&mut self) -> Result<String, String> {
        let (st, body) = self
            .get(&format!(
                "{BUY}/account/v1/editAccountFields/get?context=changeCountry"
            ))
            .await?;
        if st == 401 || st == 403 || looks_like_auth_dialog(&body) {
            return Err(session_error(&body));
        }
        if let Some(cc) = json_country(&body) {
            return Ok(cc);
        }
        let (st, body) = self.get(&format!("{BUY}/account/web/info")).await?;
        if st == 401 || st == 403 || looks_like_auth_dialog(&body) {
            return Err(session_error(&body));
        }
        json_country(&body).ok_or_else(|| "Не удалось узнать текущую страну аккаунта.".to_string())
    }

    async fn current_address(&mut self) -> Result<GeneratedAddress, String> {
        let (st, body) = self
            .get(&format!(
                "{BUY}/account/v1/editAccountFields/get?context=changeCountry"
            ))
            .await?;
        if st == 401 || st == 403 || looks_like_auth_dialog(&body) {
            return Err(session_error(&body));
        }
        let parsed: JsonValue = serde_json::from_str(&body)
            .map_err(|_| "Apple не вернул исходный платёжный адрес.".to_string())?;
        let address = parsed
            .pointer("/data/attributes/accountInfo/addressInfo")
            .unwrap_or(&parsed);
        let country_code = json_str(address, "addressOfficialCountryCode")
            .or_else(|| json_str(address, "iso3CountryCode"))
            .or_else(|| json_country(&body))
            .unwrap_or_default()
            .to_ascii_uppercase();
        let street = json_str(address, "addressOfficialLineFirst").unwrap_or_default();
        let city = json_str(address, "addressOfficialCity").unwrap_or_default();
        let postal_code = json_str(address, "addressOfficialPostalCode").unwrap_or_default();
        if country_code.len() != 3 || street.is_empty() || city.is_empty() || postal_code.is_empty()
        {
            return Err(
                "Apple не вернул полный исходный адрес. Автоматическая смена региона отменена, страна аккаунта не изменена."
                    .into(),
            );
        }
        Ok(GeneratedAddress {
            country_name: country_name(&country_code),
            country_code,
            street,
            city,
            postal_code,
            state: json_str(address, "addressOfficialStateProvince"),
            county: json_str(address, "addressOfficialCounty"),
            phone_area: json_str(address, "phoneOfficeAreaCode").unwrap_or_else(|| "000".into()),
            phone_number: json_str(address, "phoneOfficeNumber")
                .unwrap_or_else(|| "00000000".into()),
        })
    }

    fn current_names(&self, fields_json: &str) -> (String, String) {
        let parsed: JsonValue = serde_json::from_str(fields_json).unwrap_or(JsonValue::Null);
        let addr = parsed
            .pointer("/data/attributes/accountInfo/addressInfo")
            .cloned()
            .unwrap_or(JsonValue::Null);
        let info = parsed
            .pointer("/data/attributes/accountInfo/accountInfo")
            .cloned()
            .unwrap_or(JsonValue::Null);
        let first = json_str(&addr, "billingFirstName")
            .or_else(|| json_str(&info, "firstName"))
            .unwrap_or_default();
        let last = json_str(&addr, "billingLastName")
            .or_else(|| json_str(&info, "lastName"))
            .unwrap_or_default();
        (latin_name(&first, "John"), latin_name(&last, "Smith"))
    }

    async fn change_to(&mut self, address: &GeneratedAddress) -> Result<String, String> {
        let cc = address.country_code.to_ascii_uppercase();
        if is_card_only_region(&cc) {
            return Err("Для этой страны Apple просит банковскую карту. Выберите другую.".into());
        }

        let (st, current_body) = self
            .get(&format!(
                "{BUY}/account/v1/editAccountFields/get?context=changeCountry"
            ))
            .await?;
        if st == 401 || st == 403 || looks_like_auth_dialog(&current_body) {
            return Err(session_error(&current_body));
        }
        let before = json_country(&current_body).unwrap_or_default();
        if before.eq_ignore_ascii_case(&cc) {
            return Ok(cc);
        }
        let (first, last) = self.current_names(&current_body);

        let _ = self
            .get(&format!(
                "{BUY}/account/v1/editAccountFields/get?context=changeCountry&iso3CountryCode={cc}&isVideoEverywhere=true"
            ))
            .await;

        let _ = self
            .get(&format!(
                "{BUY}/account/v2/changeCountry/validate?iso3CountryCode={cc}"
            ))
            .await;

        let (st, terms) = self
            .get(&format!(
                "{AMP}/account/v1/compliance/terms/info?termsType=AMS&cc={cc}&context=changeCountry"
            ))
            .await?;
        if st == 401 || st == 403 {
            return Err(session_error(&terms));
        }
        let version_id = pick_version_id(&terms).unwrap_or(289469);
        let _ = self
            .get(&format!(
                "{AMP}/account/v1/compliance/terms/all?cc={cc}&context=changeCountry"
            ))
            .await;
        let (st, consent_body) = self
            .post(
                &format!(
                    "{AMP}/account/v1/compliance/terms/updateConsent?termsType=AMS&cc={cc}&context=changeCountry"
                ),
                &json!({ "hasConsented": true, "versionId": version_id }),
            )
            .await?;
        if st == 401 || st == 403 {
            return Err(session_error(&consent_body));
        }

        let mut payload = json!({
            "paymentMethodType": "None",
            "billingFirstName": first,
            "billingLastName": last,
            "addressOfficialLineFirst": address.street,
            "addressOfficialCity": address.city,
            "addressOfficialPostalCode": address.postal_code,
            "iso3CountryCode": cc,
            "addressOfficialCountryCode": cc,
            "phoneOfficeAreaCode": address.phone_area,
            "phoneOfficeNumber": address.phone_number,
        });
        if let Some(state) = address
            .state
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            payload["addressOfficialStateProvince"] = json!(state);
        }
        if let Some(county) = address
            .county
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            payload["addressOfficialCounty"] = json!(county);
        }

        let mut last_err = "Apple не принял новый адрес. Подождите минуту и повторите.".to_string();
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(1600)).await;
            }
            let (st, body) = self
                .post(
                    &format!("{BUY}/account/v1/nonstackable/paymentInfos/edit?"),
                    &payload,
                )
                .await?;
            if st == 401 || st == 403 || looks_like_auth_dialog(&body) {
                return Err(session_error(&body));
            }
            if st >= 400 || apple_status_failed(&body) {
                last_err = human_apple_error(&body);
                continue;
            }
            break;
        }

        let after = self.current_country().await.unwrap_or_default();
        if after.eq_ignore_ascii_case(&cc) {
            return Ok(cc);
        }
        if !after.is_empty() && !after.eq_ignore_ascii_case(&before) {
            return Ok(after);
        }
        Err(last_err)
    }
}

fn resolve_url(base: &str, loc: &str) -> String {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        loc.to_string()
    } else if let Ok(base) = reqwest::Url::parse(base) {
        base.join(loc)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| loc.to_string())
    } else {
        loc.to_string()
    }
}

fn json_str(v: &JsonValue, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn json_country(body: &str) -> Option<String> {
    for key in [
        "iTunesStoreFrontCountryCode",
        "storeFrontISO3A",
        "addressOfficialCountryCode",
    ] {
        if let Some(v) = extract_quoted(body, key) {
            if v.len() == 3 && v.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(v.to_ascii_uppercase());
            }
        }
    }
    None
}

fn extract_quoted(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let i = body.find(&pat)?;
    let rest = &body[i + pat.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn pick_version_id(body: &str) -> Option<i64> {
    if let Ok(v) = serde_json::from_str::<JsonValue>(body) {
        if let Some(n) = v.get("versionId").and_then(|x| x.as_i64()) {
            return Some(n);
        }
        if let Some(n) = v
            .pointer("/data/attributes/versionId")
            .and_then(|x| x.as_i64())
        {
            return Some(n);
        }
    }
    let key = "\"versionId\"";
    let i = body.find(key)?;
    let rest = body[i + key.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn looks_like_auth_dialog(body: &str) -> bool {
    let l = body.to_ascii_lowercase();
    l.contains("signin required")
        || l.contains("authrequired")
        || l.contains("\"kind\":\"authorization\"")
}

fn apple_status_failed(body: &str) -> bool {
    if body.trim().is_empty() {
        return false;
    }
    let Ok(value) = serde_json::from_str::<JsonValue>(body) else {
        return body.contains("\"status\":-1");
    };
    contains_apple_error(&value)
}

fn contains_apple_error(value: &JsonValue) -> bool {
    match value {
        JsonValue::Object(map) => {
            if map
                .get("status")
                .and_then(JsonValue::as_i64)
                .is_some_and(|status| status < 0)
                || map
                    .get("success")
                    .and_then(JsonValue::as_bool)
                    .is_some_and(|success| !success)
            {
                return true;
            }
            for key in [
                "errors",
                "serviceErrors",
                "validationErrors",
                "service_errors",
                "error",
                "errorMessage",
                "errorCode",
            ] {
                if let Some(entry) = map.get(key) {
                    if !entry.is_null()
                        && (!entry.is_array()
                            || entry.as_array().is_some_and(|items| !items.is_empty()))
                    {
                        return true;
                    }
                }
            }
            map.values().any(contains_apple_error)
        }
        JsonValue::Array(items) => items.iter().any(contains_apple_error),
        _ => false,
    }
}

fn collect_apple_error_text(value: &JsonValue, texts: &mut BTreeSet<String>) {
    match value {
        JsonValue::Object(map) => {
            for (key, item) in map {
                let key = key.to_ascii_lowercase();
                if matches!(
                    key.as_str(),
                    "message"
                        | "errormessage"
                        | "localizeddescription"
                        | "description"
                        | "detail"
                        | "reason"
                        | "code"
                        | "errorcode"
                        | "errortype"
                        | "statusmessage"
                ) {
                    if let Some(text) = item.as_str() {
                        let text = text.trim();
                        if !text.is_empty() {
                            texts.insert(text.to_ascii_lowercase());
                        }
                    }
                }
                collect_apple_error_text(item, texts);
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_apple_error_text(item, texts);
            }
        }
        _ => {}
    }
}

fn human_apple_error(body: &str) -> String {
    let mut texts = BTreeSet::new();
    if let Ok(value) = serde_json::from_str::<JsonValue>(body) {
        collect_apple_error_text(&value, &mut texts);
    }
    let hints = if texts.is_empty() {
        body.to_ascii_lowercase()
    } else {
        texts.into_iter().collect::<Vec<_>>().join(" ")
    };
    let has = |needles: &[&str]| needles.iter().any(|needle| hints.contains(needle));

    if has(&[
        "account balance",
        "apple account balance",
        "credit balance",
        "store credit",
        "remaining balance",
        "remaining credit",
        "gift card balance",
        "остаток средств",
    ]) {
        return "Не удалось сменить страну: на балансе Apple ID остались средства. Потратьте их и повторите. Если остатка не хватает ни на одну покупку, обратитесь в поддержку Apple.".into();
    }
    if has(&["family sharing", "family group", "семейн"]) {
        return "Не удалось сменить страну: Apple ID состоит в семейной группе. Выйдите из неё и повторите.".into();
    }
    if has(&["subscription", "apple one", "membership", "подписк"]) {
        return "Не удалось сменить страну: сначала отмените подписки, которые ещё действуют, и дождитесь окончания их периода.".into();
    }
    if has(&[
        "pre-order",
        "preorder",
        "pre order",
        "movie rental",
        "season pass",
        "pending purchase",
        "pending refund",
        "pending transaction",
        "rental",
        "refund",
        "предзаказ",
        "возврат",
        "прокат",
    ]) {
        return "Не удалось сменить страну: дождитесь завершения предзаказа, проката, покупки или возврата средств и повторите.".into();
    }
    if has(&[
        "payment verification",
        "payment method",
        "payment information",
        "billing address",
        "credit card",
        "debit card",
        "payment required",
        "card required",
        "billing",
        "платеж",
        "карт",
    ]) {
        return "Для выбранной страны Apple просит подходящий способ оплаты и платёжный адрес. Проверьте их в Apple ID и повторите.".into();
    }
    if has(&[
        "postal",
        "post code",
        "postcode",
        "zip code",
        "zip",
        "state/province",
        "address invalid",
        "индекс",
    ]) {
        return "Apple не принял адрес для выбранной страны. Нажмите «Другой адрес» и повторите.".into();
    }
    if has(&["terms", "consent", "agreement", "услови", "соглашен"]) {
        return "Apple просит подтвердить условия для выбранной страны. Подтвердите их в Apple ID и повторите.".into();
    }
    if has(&[
        "try again",
        "later",
        "temporarily",
        "unavailable",
        "rate limit",
        "too many",
        "timeout",
        "network",
        "connection",
        "service error",
        "временно",
        "попробуйте позже",
    ]) {
        return "Сейчас Apple временно не принимает смену страны. Подождите пару минут и попробуйте снова.".into();
    }
    "Не удалось сменить страну. Подождите минуту и попробуйте снова.".into()
}

fn session_error(body: &str) -> String {
    let _ = body;
    "Вход в Apple ID истёк. Войдите снова и повторите смену страны.".into()
}

impl CommerceClient {
    async fn accept_media_terms(&mut self) -> Result<(), String> {
        let cc = self
            .current_country()
            .await
            .unwrap_or_else(|_| "RUS".into());
        let (_, info) = self
            .get(&format!(
                "{AMP}/account/v1/compliance/terms/info?termsType=AMS&cc={cc}"
            ))
            .await?;
        let vid = pick_version_id(&info).unwrap_or(289404);
        let _ = self
            .post(
                &format!("{AMP}/account/v1/compliance/terms/updateConsent?termsType=AMS&cc={cc}"),
                &json!({ "hasConsented": true, "versionId": vid }),
            )
            .await;
        Ok(())
    }
}

                                                                              
                                                                             
pub async fn accept_pending_media_terms(email: &str) -> Result<(), String> {
    // Never start a fresh Apple ID login here: idmsa would push a 2FA code
    // to the iPhone while ipatool download/install already works.
    match saved_commerce_client(email).await {
        Ok(mut client) => client.accept_media_terms().await,
        Err(_) => Ok(()),
    }
}

async fn saved_commerce_client(email: &str) -> Result<CommerceClient, String> {
    let auth = crate::history_secrets::load_region_session(email)
        .ok_or_else(|| "Подтвердите вход в Apple ID и повторите.".to_string())?;
    let mut client = CommerceClient::new(auth)?;
    client.establish_store_session().await?;
    let _ = crate::history_secrets::store_region_session(email, &client.account_auth_session());
    Ok(client)
}

pub async fn snapshot_current_region(email: &str) -> Result<GeneratedAddress, String> {
    let mut client = saved_commerce_client(email).await?;
    client.current_address().await
}

pub async fn switch_saved_region(email: &str, address: &GeneratedAddress) -> Result<(), String> {
    let mut client = saved_commerce_client(email).await?;
    client.change_to(address).await?;
    let _ = crate::history_secrets::store_region_session(email, &client.account_auth_session());
    Ok(())
}

pub async fn restore_temporary_region(
    email: &str,
    original: &GeneratedAddress,
) -> Result<(), String> {
    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(1800)).await;
        }
        match saved_commerce_client(email).await {
            Ok(mut client) => match client.change_to(original).await {
                Ok(_) => {
                    let _ = crate::history_secrets::store_region_session(
                        email,
                        &client.account_auth_session(),
                    );
                    return Ok(());
                }
                Err(error) => last_error = error,
            },
            Err(error) => last_error = error,
        }
    }
    Err(format!(
        "Не удалось вернуть регион {} после трёх попыток: {last_error}",
        original.country_name
    ))
}

async fn client_from_saved_or_auth(
    email: &str,
    password: Option<&str>,
    two_factor: Option<&str>,
    pending: Option<crate::apple_history::PendingHistoryAuth>,
) -> Result<Result<CommerceClient, crate::apple_history::PendingHistoryAuth>, String> {
    if two_factor.is_none() {
        if let Some(auth) = crate::history_secrets::load_region_session(email) {
            if let Ok(mut client) = CommerceClient::new(auth) {
                if client.establish_store_session().await.is_ok() {
                    let _ = crate::history_secrets::store_region_session(
                        email,
                        &client.account_auth_session(),
                    );
                    return Ok(Ok(client));
                }
            }
        }
    }

    let provided = password.unwrap_or("").trim();
    let resolved = crate::history_secrets::resolve_history_password(email, provided)?;
    match crate::apple_history::authenticate_region_account(email, &resolved, two_factor, pending)
        .await
    {
        Ok(Err(pending)) => Ok(Err(pending)),
        Ok(Ok(auth)) => {
            let _ = crate::history_secrets::store_history_password(email, &resolved);
            let mut client = CommerceClient::new(auth)?;
            client.establish_store_session().await?;
            let _ =
                crate::history_secrets::store_region_session(email, &client.account_auth_session());
            Ok(Ok(client))
        }
        Err(e) => Err(e.user_message()),
    }
}

pub async fn load_store_region(
    email: &str,
    password: Option<&str>,
    two_factor: Option<&str>,
    pending: Option<crate::apple_history::PendingHistoryAuth>,
) -> Result<Result<StoreRegionStatus, crate::apple_history::PendingHistoryAuth>, String> {
    match client_from_saved_or_auth(email, password, two_factor, pending).await? {
        Err(pending) => Ok(Err(pending)),
        Ok(mut client) => {
            let code = client.current_country().await?;
            Ok(Ok(StoreRegionStatus {
                country_name: country_name(&code),
                country_code: code,
                needs_2fa: false,
                message: None,
            }))
        }
    }
}

pub async fn apply_store_region(
    email: &str,
    country: &str,
    address: Option<GeneratedAddress>,
    password: Option<&str>,
    two_factor: Option<&str>,
    pending: Option<crate::apple_history::PendingHistoryAuth>,
) -> Result<Result<ChangeRegionResult, crate::apple_history::PendingHistoryAuth>, String> {
    let addr = match address {
        Some(a) if a.country_code.eq_ignore_ascii_case(country) => a,
        _ => generate_address(country)?,
    };
    match client_from_saved_or_auth(email, password, two_factor, pending).await? {
        Err(pending) => Ok(Err(pending)),
        Ok(mut client) => {
            let code = client.change_to(&addr).await?;
            let _ =
                crate::history_secrets::store_region_session(email, &client.account_auth_session());
            Ok(Ok(ChangeRegionResult {
                ok: true,
                needs_2fa: false,
                country_name: country_name(&code),
                country_code: code,
                message: "Страна магазина обновлена.".into(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn auth_session() -> crate::apple_history::AccountAuthSession {
        crate::apple_history::AccountAuthSession {
            cookies: HashMap::from([("myacinfo".into(), "cookie".into())]),
            scnt: Some("scnt-value".into()),
            session_id: Some("session-id".into()),
            session_token: Some("session-token".into()),
            repair_session_token: Some("repair-token".into()),
            auth_attributes: Some("auth-attributes".into()),
        }
    }

    #[test]
    fn regions_have_unique_codes_and_names() {
        let list = list_store_regions();
        assert!(list.len() >= 10);
        let mut codes = HashSet::new();
        for r in &list {
            assert_eq!(r.code.len(), 3);
            assert!(codes.insert(r.code.clone()), "duplicate {}", r.code);
            assert!(!r.name.is_empty());
        }
    }

    #[test]
    fn usa_zip_matches_state() {
        for row in ADDRESSES.iter().filter(|r| r.iso3 == "USA") {
            assert!(row.state.is_some(), "USA needs state");
            assert_eq!(row.postal.len(), 5);
            assert!(row.postal.chars().all(|c| c.is_ascii_digit()));
            assert_eq!(row.state.unwrap().len(), 2);
        }
        let a = generate_address("USA").unwrap();
        assert_eq!(a.country_code, "USA");
        assert!(a.state.is_some());
        assert_eq!(a.postal_code.len(), 5);
    }

    #[test]
    fn region_address_list_exposes_stable_choices() {
        let usa = list_region_addresses("usa").unwrap();
        assert_eq!(usa.len(), 10);
        assert!(usa.iter().all(|address| address.country_code == "USA"));
        assert_eq!(usa[0].street, "350 5th Avenue");
        let russia = list_region_addresses("rus").unwrap();
        assert_eq!(russia.len(), 10);
        assert!(russia.iter().all(|address| address.country_code == "RUS"));
        assert!(list_store_regions()
            .iter()
            .any(|region| region.code == "RUS"));
        assert!(list_region_addresses("DEU").is_err());
    }

    #[test]
    fn every_selectable_region_has_ten_distinct_addresses() {
        for region in list_store_regions() {
            let addresses = list_region_addresses(&region.code).unwrap();
            assert_eq!(addresses.len(), 10, "{}", region.code);

            let cities: HashSet<_> = addresses.iter().map(|address| &address.city).collect();
            let streets: HashSet<_> = addresses.iter().map(|address| &address.street).collect();
            let postals: HashSet<_> = addresses
                .iter()
                .map(|address| &address.postal_code)
                .collect();
            assert_eq!(cities.len(), 10, "{} has duplicate cities", region.code);
            assert_eq!(streets.len(), 10, "{} has duplicate streets", region.code);
            assert_eq!(
                postals.len(),
                10,
                "{} has duplicate postal codes",
                region.code
            );
        }
    }

    #[test]
    fn verified_ny_address_present() {
        assert!(ADDRESSES.iter().any(|r| {
            r.iso3 == "USA" && r.state == Some("NY") && r.postal == "10118" && r.city == "New York"
        }));
    }

    #[test]
    fn eu_rejected() {
        assert!(generate_address("DEU").is_err());
        assert!(generate_address("ROU").is_err());
        assert!(is_card_only_region("FRA"));
    }

    #[test]
    fn json_country_parses_web_info() {
        let body = r#"{"storeFrontISO3A":"USA","storeFrontId":143441}"#;
        assert_eq!(json_country(body).as_deref(), Some("USA"));
        let fields = r#"{"data":{"attributes":{"accountInfo":{"accountInfo":{"iTunesStoreFrontCountryCode":"RUS"}}}}}"#;
        assert_eq!(json_country(fields).as_deref(), Some("RUS"));
    }

    #[test]
    fn version_id_from_terms() {
        assert_eq!(pick_version_id(r#"{"versionId":289469}"#), Some(289469));
    }

    #[test]
    fn country_change_errors_read_nested_apple_error_shapes() {
        let balance = r#"{"serviceErrors":[{"code":"BALANCE_REMAINING","message":"Spend your remaining account balance before changing country."}]}"#;
        assert!(apple_status_failed(balance));
        assert!(human_apple_error(balance).contains("остались средства"));

        let subscription = r#"{"validationErrors":[{"errorCode":"ACTIVE_SUBSCRIPTION","localizedDescription":"Cancel subscriptions before changing your region."}]}"#;
        assert!(apple_status_failed(subscription));
        assert!(human_apple_error(subscription).contains("отмените подписки"));

        let family = r#"{"service_errors":[{"reason":"You are part of a Family Sharing group."}]}"#;
        assert!(apple_status_failed(family));
        assert!(human_apple_error(family).contains("семейной группе"));
    }

    #[test]
    fn country_change_errors_explain_pending_and_payment_requirements() {
        let pending = r#"{"errors":[{"message":"Wait until your pending refund is processed."}]}"#;
        assert!(human_apple_error(pending).contains("дождитесь завершения"));

        let payment = r#"{"errorMessage":"Payment verification is required for this country."}"#;
        assert!(human_apple_error(payment).contains("способ оплаты"));

        let address = r#"{"error":{"detail":"The postal code does not match the selected state."}}"#;
        assert!(human_apple_error(address).contains("Другой адрес"));
    }

    #[test]
    fn store_requests_keep_post_2fa_auth_metadata() {
        let client = CommerceClient::new(auth_session()).unwrap();
        let mut headers = HeaderMap::new();
        client.apply_auth_headers(&mut headers);

        assert_eq!(headers["scnt"], "scnt-value");
        assert_eq!(headers["X-Apple-ID-Session-Id"], "session-id");
        assert_eq!(headers["X-Apple-Session-Token"], "session-token");
        assert_eq!(headers["X-Apple-Repair-Session-Token"], "repair-token");
        assert_eq!(headers["X-Apple-Auth-Attributes"], "auth-attributes");
    }

    #[test]
    fn updated_store_cookies_do_not_discard_auth_metadata() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        client
            .cookies
            .map
            .insert("store-cookie".into(), "updated".into());

        let saved = client.account_auth_session();
        assert_eq!(
            saved.cookies.get("store-cookie").map(String::as_str),
            Some("updated")
        );
        assert_eq!(saved.session_token.as_deref(), Some("session-token"));
        assert_eq!(saved.auth_attributes.as_deref(), Some("auth-attributes"));
    }

    #[test]
    fn account_bootstrap_replaces_stale_auth_metadata() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("scnt", HeaderValue::from_static("fresh-scnt"));
        headers.insert(
            "x-apple-id-session-id",
            HeaderValue::from_static("fresh-session-id"),
        );
        headers.insert(
            "x-apple-auth-attributes",
            HeaderValue::from_static("fresh-auth-attributes"),
        );

        client.ingest_auth_headers(&headers);

        assert_eq!(client.auth.scnt.as_deref(), Some("fresh-scnt"));
        assert_eq!(client.auth.session_id.as_deref(), Some("fresh-session-id"));
        assert_eq!(
            client.auth.auth_attributes.as_deref(),
            Some("fresh-auth-attributes")
        );
    }

    #[test]
    fn commerce_cookie_header_matches_browser_scope() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        client.cookies.map.insert("site".into(), "RUS".into());
        client.cookies.map.insert("aasp".into(), "idms-only".into());
        client.ingest_gs_body(
            r#"{"pldfltcid":"private-device","pltvcid":"private-video","itspod":"15"}"#,
        );

        assert_eq!(client.pldfltcid.as_deref(), Some("private-device"));
        assert_eq!(client.pltvcid.as_deref(), Some("private-video"));
        assert_eq!(
            client.cookies.map.get("itspod").map(String::as_str),
            Some("15")
        );
        client.cookies.map.insert("caw".into(), "caw-value".into());
        client
            .cookies
            .map
            .insert("commerce-authorization-token".into(), "store-token".into());
        let header = client.cookies.commerce_header_value().unwrap();
        assert!(header.contains("itspod=15"));
        assert!(header.contains("site=RUS"));
        assert!(header.contains("myacinfo=cookie"));
        assert!(header.contains("caw=caw-value"));
        assert!(header.contains("commerce-authorization-token=store-token"));
        assert!(!header.contains("aasp="));
        assert!(!header.contains("pldfltcid="));
        assert!(!header.contains("pltvcid="));
    }

    #[test]
    fn packed_gs_set_cookie_keeps_commerce_tokens() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static(
                "commerce-authorization-token=tokA; Domain=.apps.apple.com; Path=/; Secure, itspod=15; Domain=.apple.com; Path=/, commerce-authorization-token-lite=lite; Domain=.apps.apple.com; Path=/, commerce-on-web-token=web; Path=/, commerce-ds-session-id=ds; Path=/",
            ),
        );
        client.cookies.store_from_response(&headers);
        let header = client.cookies.commerce_header_value().unwrap();
        assert!(header.contains("commerce-authorization-token=tokA"));
        assert!(header.contains("itspod=15"));
        assert!(header.contains("commerce-authorization-token-lite=lite"));
        assert!(header.contains("commerce-on-web-token=web"));
        assert!(header.contains("commerce-ds-session-id=ds"));
    }

    #[test]
    fn empty_gs_delete_cookie_does_not_drop_token() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static("commerce-authorization-token=tokA; Domain=.apps.apple.com"),
        );
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static(
                "commerce-authorization-token=; Version=1; Domain=.apple.com; Max-Age=0",
            ),
        );
        client.cookies.store_from_response(&headers);
        assert_eq!(
            client
                .cookies
                .map
                .get("commerce-authorization-token")
                .map(String::as_str),
            Some("tokA")
        );
    }

    #[test]
    fn site_cookie_selects_usa_store_front() {
        let mut cookies = HashMap::from([("myacinfo".into(), "cookie".into())]);
        cookies.insert("site".into(), "USA".into());
        let auth = crate::apple_history::AccountAuthSession {
            cookies,
            scnt: None,
            session_id: None,
            session_token: None,
            repair_session_token: None,
            auth_attributes: None,
        };
        let client = CommerceClient::new(auth).unwrap();
        assert_eq!(client.store_front.as_deref(), Some("143441-1,8"));
    }

    #[test]
    fn initial_store_front_uses_browser_header_format() {
        let mut client = CommerceClient::new(auth_session()).unwrap();
        assert_eq!(client.store_front.as_deref(), Some("143469-1,8"));
        client.set_store_front_country("USA");
        assert_eq!(client.store_front.as_deref(), Some("143441-1,8"));
    }
}
