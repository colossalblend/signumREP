#![allow(dead_code, unused_imports)]

#[path = "vendor/apple_history.rs"]
mod apple_history;
#[path = "vendor/apple_region.rs"]
mod apple_region;

use apple_history::{HistoryOutcome, HistorySuccess, PendingHistoryAuth};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Mutex;

mod diag_log {
    pub fn http(_: &str, _: &str, _: u16, _: &str, _: &[u8]) {}
    pub fn http_err(_: &str, _: &str, _: &str) {}
}

mod history_secrets {
    use super::*;
    static SESSION: Mutex<Option<apple_history::AccountAuthSession>> = Mutex::new(None);
    pub fn load_region_session(_: &str) -> Option<apple_history::AccountAuthSession> {
        SESSION.lock().ok()?.clone()
    }
    pub fn store_region_session(
        _: &str,
        session: &apple_history::AccountAuthSession,
    ) -> Result<(), String> {
        *SESSION.lock().map_err(|_| "session_unavailable")? = Some(session.clone());
        Ok(())
    }
    pub fn resolve_history_password(_: &str, password: &str) -> Result<String, String> {
        if password.is_empty() {
            Err("password_required".into())
        } else {
            Ok(password.into())
        }
    }
    pub fn store_history_password(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
struct SavedState {
    #[serde(default)]
    history_cookies: HashMap<String, String>,
    next_batch: Option<String>,
    dsid: Option<String>,
    xsrf: Option<String>,
    region_session: Option<apple_history::AccountAuthSession>,
}

#[derive(Deserialize)]
struct Request {
    action: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    code: Option<String>,
    country: Option<String>,
    address: Option<apple_region::GeneratedAddress>,
    state: Option<SavedState>,
}

struct Bridge {
    email: Option<String>,
    saved: SavedState,
    pending: Option<PendingHistoryAuth>,
    pending_action: String,
}

impl Bridge {
    fn history_result(&mut self, success: HistorySuccess) -> Value {
        let apps: Vec<Value> = success
            .apps
            .iter()
            .map(|app| {
                json!({
                    "id": app.adam_id, "name": app.name, "artist": app.artist,
                    "artwork": app.artwork, "price": app.price, "date": app.date
                })
            })
            .collect();
        self.saved.history_cookies = success.cookies;
        self.saved.next_batch = success.next_batch_id;
        self.saved.dsid = success.rap_dsid;
        self.saved.xsrf = success.rap_xsrf;
        self.pending = None;
        json!({"status":"ready", "apps": apps, "has_more":success.has_more})
    }

    async fn history(&mut self, request: &Request) -> Value {
        if request.code.is_none() && !self.saved.history_cookies.is_empty() {
            let cursor = if request.action == "history_next" {
                self.saved.next_batch.clone()
            } else {
                None
            };
            let creds = self.saved.dsid.clone().zip(self.saved.xsrf.clone());
            if let Ok(success) = apple_history::fetch_history_with_cookies(
                &self.saved.history_cookies,
                1,
                cursor,
                creds,
            )
            .await
            {
                return self.history_result(success);
            }
        }
        let pending = self.pending.clone();
        match apple_history::fetch_purchase_history(
            &request.email,
            &request.password,
            request.code.as_deref(),
            pending,
        )
        .await
        {
            Ok(HistoryOutcome::Apps(success)) => self.history_result(success),
            Ok(HistoryOutcome::Needs2fa { pending }) => {
                self.pending = Some(pending);
                self.pending_action = request.action.clone();
                json!({"status":"requires_2fa"})
            }
            Err(error) if error.keep_pending() && self.pending.is_some() => {
                json!({"status":"requires_2fa", "error":"invalid_code"})
            }
            Err(error) => {
                self.pending = None;
                let code = match error {
                    apple_history::HistoryError::WrongPassword => "wrong_password",
                    apple_history::HistoryError::AccountLocked => "account_locked",
                    _ => "apple_unavailable",
                };
                json!({"status":"failed", "error":code})
            }
        }
    }

    async fn region(&mut self, request: &Request) -> Value {
        let pending = self.pending.clone();
        let result = if request.action == "region_change" {
            apple_region::apply_store_region(
                &request.email,
                request.country.as_deref().unwrap_or(""),
                request.address.clone(),
                Some(&request.password),
                request.code.as_deref(),
                pending,
            )
            .await
            .map(|value| value.map(|result| serde_json::to_value(result).unwrap()))
        } else {
            apple_region::load_store_region(
                &request.email,
                Some(&request.password),
                request.code.as_deref(),
                pending,
            )
            .await
            .map(|value| value.map(|result| serde_json::to_value(result).unwrap()))
        };
        match result {
            Ok(Ok(value)) => {
                self.pending = None;
                json!({"status":"ready", "region":value})
            }
            Ok(Err(pending)) => {
                self.pending = Some(pending);
                self.pending_action = request.action.clone();
                json!({"status":"requires_2fa"})
            }
            Err(message) => {
                if self.pending.is_some() && request.code.is_some() && message.contains("Код") {
                    return json!({"status":"requires_2fa", "error":"invalid_code"});
                }
                self.pending = None;
                let lower = message.to_lowercase();
                let code = if lower.contains("подпис") {
                    "active_subscriptions"
                } else if lower.contains("баланс") || lower.contains("credit") {
                    "remaining_balance"
                } else if lower.contains("семейн") || lower.contains("family") {
                    "family_sharing"
                } else if lower.contains("платеж")
                    || lower.contains("оплат")
                    || lower.contains("карт")
                {
                    "payment_required"
                } else if lower.contains("парол") {
                    "wrong_password"
                } else {
                    "region_unavailable"
                };
                json!({"status":"failed", "error":code})
            }
        }
    }

    async fn handle(&mut self, mut request: Request) -> Value {
        if request.action == "regions" {
            return json!({"status":"ready", "regions":apple_region::list_store_regions()});
        }
        if request.action == "addresses" {
            return match apple_region::list_region_addresses(
                request.country.as_deref().unwrap_or(""),
            ) {
                Ok(addresses) => json!({"status":"ready", "addresses":addresses}),
                Err(_) => json!({"status":"failed", "error":"invalid_country"}),
            };
        }
        if self
            .email
            .as_ref()
            .is_some_and(|email| email != &request.email)
        {
            return json!({"status":"failed", "error":"account_mismatch"});
        }
        self.email = Some(request.email.clone());
        if request.code.is_some()
            && (self.pending.is_none() || self.pending_action != request.action)
        {
            return json!({"status":"failed", "error":"code_not_required"});
        }
        if let Some(saved) = request.state.take() {
            if let Some(session) = saved.region_session.as_ref() {
                let _ = history_secrets::store_region_session(&request.email, session);
            }
            self.saved = saved;
        }
        let result = if request.action.starts_with("history") {
            self.history(&request).await
        } else if request.action.starts_with("region_") {
            self.region(&request).await
        } else {
            json!({"status":"failed", "error":"invalid_action"})
        };
        self.saved.region_session = history_secrets::load_region_session(&request.email);
        let mut response = result;
        response["state"] = serde_json::to_value(&self.saved).unwrap();
        response
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut bridge = Bridge {
        email: None,
        saved: SavedState::default(),
        pending: None,
        pending_action: String::new(),
    };
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.len() > 4 * 1024 * 1024 {
            break;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => bridge.handle(request).await,
            Err(_) => json!({"status":"failed", "error":"invalid_request"}),
        };
        println!("{}", response);
        if io::stdout().flush().is_err() {
            break;
        }
    }
}
