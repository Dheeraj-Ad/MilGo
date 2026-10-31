use axum::{extract::{State, Path, Query, Request, DefaultBodyLimit}, http::{HeaderMap, HeaderValue, StatusCode, header}, middleware::{self, Next}, response::{IntoResponse, Response, Redirect}, routing::{get, post, delete}, Json, Router};
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::{SaltString, PasswordHash}};
use base64::{Engine, engine::general_purpose::{URL_SAFE_NO_PAD, STANDARD}};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::{Aead, Payload}};
use rand::{RngCore, rngs::OsRng};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Sha256, Digest};
use std::{sync::{Arc, Mutex, MutexGuard}, time::{SystemTime, UNIX_EPOCH, Duration}, collections::HashMap, net::SocketAddr};
use tower_http::services::{ServeDir, ServeFile};
mod activation;

type Result<T> = std::result::Result<T, AppError>;
#[derive(Debug)]
struct AppError(StatusCode, String);
impl IntoResponse for AppError { fn into_response(self)->Response { (self.0, Json(json!({"error":self.1}))).into_response() } }
impl From<rusqlite::Error> for AppError { fn from(e:rusqlite::Error)->Self { tracing::error!(error=%e,"database operation failed"); Self(StatusCode::INTERNAL_SERVER_ERROR,"The town ledger is unavailable. Please try again.".into()) } }
impl From<reqwest::Error> for AppError { fn from(_:reqwest::Error)->Self { Self(StatusCode::BAD_GATEWAY,"Google could not be reached. Please try again.".into()) } }
fn bad(message:&str)->AppError { AppError(StatusCode::BAD_REQUEST,message.into()) }
fn unauthorized()->AppError { AppError(StatusCode::UNAUTHORIZED,"Please sign in to continue.".into()) }
fn now()->i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 }
fn random()->String { let mut bytes=[0u8;32];OsRng.fill_bytes(&mut bytes);URL_SAFE_NO_PAD.encode(bytes) }
fn digest(s:&str)->String { URL_SAFE_NO_PAD.encode(Sha256::digest(s.as_bytes())) }

#[derive(Clone)]
struct AppState { db:Arc<Mutex<Connection>>, client:reqwest::Client, config:Arc<Config>, cipher:Arc<ChaCha20Poly1305>, otp_key:[u8;32], limits:Arc<Mutex<HashMap<String,(i64,u32)>>>, mail_lock:Arc<tokio::sync::Mutex<()>>, #[cfg(test)] test_outbox:Arc<Mutex<Vec<(String,String)>>>, #[cfg(test)] test_gmail_base:Option<String> }
struct Config { origin:String, google_id:String, google_secret:String, secure:bool, otp_api_key:String, otp_from:String, otp_api_url:String }
impl AppState {
 fn gmail_base(&self)->&str {#[cfg(test)]if let Some(base)=&self.test_gmail_base{return base;}GMAIL}
 fn db(&self)->Result<MutexGuard<'_,Connection>> {self.db.lock().map_err(|_|AppError(StatusCode::INTERNAL_SERVER_ERROR,"Database unavailable.".into()))}
 fn seal(&self,user:&str,plain:&str)->Result<String>{let mut nonce=[0;12];OsRng.fill_bytes(&mut nonce);let encrypted=self.cipher.encrypt(Nonce::from_slice(&nonce),Payload{msg:plain.as_bytes(),aad:user.as_bytes()}).map_err(|_|bad("Could not protect credentials."))?;let mut bytes=nonce.to_vec();bytes.extend(encrypted);Ok(URL_SAFE_NO_PAD.encode(bytes))}
 fn unseal(&self,user:&str,value:&str)->Result<String>{let bytes=URL_SAFE_NO_PAD.decode(value).map_err(|_|bad("Please reconnect Gmail."))?;if bytes.len()<13{return Err(bad("Please reconnect Gmail."));}let plain=self.cipher.decrypt(Nonce::from_slice(&bytes[..12]),Payload{msg:&bytes[12..],aad:user.as_bytes()}).map_err(|_|bad("Please reconnect Gmail."))?;String::from_utf8(plain).map_err(|_|bad("Please reconnect Gmail."))}
}

fn initialize(db:&Connection)->rusqlite::Result<()> {db.execute_batch("
 PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
 CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,password TEXT,google_sub TEXT UNIQUE,terms INTEGER NOT NULL DEFAULT 0,xp INTEGER NOT NULL DEFAULT 0,completed INTEGER NOT NULL DEFAULT 0);
 CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS google_tokens(user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,access TEXT NOT NULL,refresh TEXT,expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS oauth_states(state_hash TEXT PRIMARY KEY,verifier TEXT NOT NULL,user_id TEXT REFERENCES users(id) ON DELETE CASCADE,expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS actions(user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,message_id TEXT NOT NULL,action TEXT NOT NULL,status TEXT NOT NULL,created INTEGER NOT NULL,PRIMARY KEY(user_id,message_id,action));
 ")?; activation::migrate(db) }

#[derive(Serialize,Debug)]
#[serde(rename_all="camelCase")]
struct User {id:String,email:String,name:String,gmail:bool,terms:bool,xp:i64,completed:i64,expires_at:Option<i64>,last_synced_at:Option<i64>,sync_error:Option<String>}
fn user_by_id(s:&AppState,id:&str)->Result<User>{s.db()?.query_row("SELECT u.id,u.email,u.name,EXISTS(SELECT 1 FROM google_tokens WHERE user_id=u.id),u.terms,u.xp,u.completed,a.expires,a.last_synced,a.sync_error FROM users u LEFT JOIN activations a ON a.user_id=u.id AND a.expires>?2 WHERE u.id=?1",params![id,now()],|r|Ok(User{id:r.get(0)?,email:r.get(1)?,name:r.get(2)?,gmail:r.get(3)?,terms:r.get(4)?,xp:r.get(5)?,completed:r.get(6)?,expires_at:r.get(7)?,last_synced_at:r.get(8)?,sync_error:r.get(9)?})).optional()?.ok_or_else(unauthorized)}
fn cookie(headers:&HeaderMap,name:&str)->Option<String>{headers.get(header::COOKIE)?.to_str().ok()?.split(';').find_map(|v|{let(k,v)=v.trim().split_once('=')?;(k==name).then(||v.to_string())})}
fn current(s:&AppState,h:&HeaderMap)->Result<User>{let raw=cookie(h,"tq_session").ok_or_else(unauthorized)?;let id:String=s.db()?.query_row("SELECT s.user_id FROM sessions s JOIN activations a ON a.user_id=s.user_id AND a.generation=s.activation_id WHERE s.token_hash=?1 AND s.expires>?2 AND a.expires>?2",params![digest(&raw),now()],|r|r.get(0)).optional()?.ok_or_else(unauthorized)?;user_by_id(s,&id)}
fn resident(s:&AppState,h:&HeaderMap)->Result<User>{let u=current(s,h)?;if !u.terms{return Err(AppError(StatusCode::FORBIDDEN,"Accept the terms to continue.".into()));}Ok(u)}
fn set_cookie(s:&AppState,name:&str,value:&str,age:i64)->HeaderValue{HeaderValue::from_str(&format!("{name}={value}; HttpOnly; Path=/; SameSite=Lax; Max-Age={age}{}",if s.config.secure{"; Secure"}else{""})).expect("safe generated cookie")}

async fn security(State(s):State<AppState>,req:Request,next:Next)->Response {
 let method=req.method().clone();let path=req.uri().path().to_string();
 if !matches!(method,axum::http::Method::GET|axum::http::Method::HEAD|axum::http::Method::OPTIONS){
  if req.headers().get("x-taskocalypse").and_then(|v|v.to_str().ok())!=Some("town"){return AppError(StatusCode::FORBIDDEN,"Request verification failed.".into()).into_response();}
  if let Some(origin)=req.headers().get(header::ORIGIN){if origin.to_str().ok()!=Some(s.config.origin.as_str()){return AppError(StatusCode::FORBIDDEN,"This origin is not allowed.".into()).into_response();}}
 }
 if path.starts_with("/api/auth/")&&method==axum::http::Method::POST {
  let key=req.extensions().get::<axum::extract::ConnectInfo<SocketAddr>>().map(|ip|ip.0.ip().to_string()).unwrap_or_else(||"local".into());
  if let Ok(mut limits)=s.limits.lock(){limits.retain(|_,(t,_)|now()-*t<60);let entry=limits.entry(key).or_insert((now(),0));entry.1+=1;if entry.1>20{return AppError(StatusCode::TOO_MANY_REQUESTS,"Take a short breather and try again in a minute.".into()).into_response();}}
 }
 let mut response=next.run(req).await;let h=response.headers_mut();h.insert("x-content-type-options",HeaderValue::from_static("nosniff"));h.insert("referrer-policy",HeaderValue::from_static("no-referrer"));h.insert("x-frame-options",HeaderValue::from_static("DENY"));
 if path.starts_with("/api/"){h.insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store"));}
 response
}
#[derive(Deserialize)]struct Credentials{email:String,password:String,#[serde(default)]name:String,#[serde(default)]terms:bool}
fn normalize_email(email:&str)->Result<String>{let email=email.trim().to_lowercase();if email.len()>254||email.contains(char::is_whitespace)||!email.split_once('@').is_some_and(|(a,b)|!a.is_empty()&&b.contains('.')&&!b.starts_with('.')&&!b.ends_with('.')){return Err(bad("Enter a valid email address."));}Ok(email)}
async fn register(State(s):State<AppState>,h:HeaderMap,Json(c):Json<Credentials>)->Result<Response>{
 if !c.terms{return Err(bad("Please accept the terms first."));}let email=normalize_email(&c.email)?;if !(12..=128).contains(&c.password.len()){return Err(bad("Choose a password between 12 and 128 bytes."));}let name=c.name.trim().to_string();if name.chars().count()<2||name.chars().count()>60{return Err(bad("Enter a name between 2 and 60 characters."));}
 activation::ensure_mailer(&s)?; let _=h;
 let password=tokio::task::spawn_blocking(move||Argon2::default().hash_password(c.password.as_bytes(),&SaltString::generate(&mut OsRng)).map(|h|h.to_string())).await.map_err(|_|bad("Could not create account."))?.map_err(|_|bad("Could not protect password."))?;
 let id=uuid::Uuid::new_v4().to_string();let result=s.db()?.execute("INSERT INTO users(id,email,name,password,terms) VALUES(?1,?2,?3,?4,?5)",params![id,email,name,password,now()]);
 match result{Ok(_)=>activation::challenge(&s,&id,&email,None).await,Err(rusqlite::Error::SqliteFailure(e,_))if e.code==rusqlite::ErrorCode::ConstraintViolation=>Err(bad("An account already exists. Please sign in.")),Err(e)=>Err(e.into())}
}
async fn login(State(s):State<AppState>,h:HeaderMap,Json(c):Json<Credentials>)->Result<Response>{
 if !c.terms{return Err(bad("Accept the terms before signing in."));} activation::ensure_mailer(&s)?;let _=h;
 let email=normalize_email(&c.email)?;if c.password.len()>128{return Err(unauthorized());}
 let entry:Option<(String,Option<String>)>=s.db()?.query_row("SELECT id,password FROM users WHERE email=?1",[email],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
 let (id,hash)=entry.unwrap_or_default();let valid=tokio::task::spawn_blocking(move||{if let Some(hash)=hash{PasswordHash::new(&hash).ok().is_some_and(|parsed|Argon2::default().verify_password(c.password.as_bytes(),&parsed).is_ok())}else{let _=Argon2::default().hash_password(c.password.as_bytes(),&SaltString::generate(&mut OsRng));false}}).await.unwrap_or(false);
 if !valid{return Err(AppError(StatusCode::UNAUTHORIZED,"Email or password is incorrect. Google accounts should use Continue with Google.".into()));}let u=user_by_id(&s,&id)?;activation::challenge(&s,&id,&u.email,None).await
}
async fn me(State(s):State<AppState>,h:HeaderMap)->Result<Json<User>>{Ok(Json(current(&s,&h)?))}
async fn terms(State(s):State<AppState>,h:HeaderMap)->Result<Json<Value>>{let u=current(&s,&h)?;s.db()?.execute("UPDATE users SET terms=?1 WHERE id=?2",params![now(),u.id])?;Ok(Json(json!({"ok":true})))}
async fn logout(State(s):State<AppState>,h:HeaderMap)->Result<Response>{let _guard=s.mail_lock.lock().await;if let Ok(u)=current(&s,&h){let mut db=s.db()?;let tx=db.transaction()?;activation::terminate(&tx,&u.id)?;tx.commit()?;}if let Some(pending)=cookie(&h,"tq_pending"){s.db()?.execute("DELETE FROM email_challenges WHERE token_hash=?1",[digest(&pending)])?;}let mut r=Json(json!({"ok":true})).into_response();for name in ["tq_session","tq_pending","tq_oauth"]{r.headers_mut().append(header::SET_COOKIE,set_cookie(&s,name,"",0));}Ok(r)}
async fn remove_account(State(s):State<AppState>,h:HeaderMap)->Result<Response>{let u=current(&s,&h)?;{let _guard=s.mail_lock.lock().await;revoke(&s,&u.id).await;s.db()?.execute("DELETE FROM users WHERE id=?1",[u.id])?;}logout(State(s),h).await}

#[derive(Deserialize)]struct Consent{terms:bool}
async fn google_start(State(s):State<AppState>,h:HeaderMap,Json(c):Json<Consent>)->Result<Response>{
 if !c.terms{return Err(bad("Accept the terms first."));}if s.config.google_id.is_empty()||s.config.google_secret.is_empty(){return Err(AppError(StatusCode::SERVICE_UNAVAILABLE,"Google sign-in needs OAuth credentials. Follow SETUP.md to configure GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET on the Rust server.".into()));}
 activation::ensure_mailer(&s)?;
 let state=random();let verifier=random();let user=current(&s,&h).ok().map(|u|u.id);let hash=digest(&state);let encrypted=s.seal(&hash,&verifier)?;
 {let db=s.db()?;db.execute("DELETE FROM oauth_states WHERE expires<?1",[now()])?;db.execute("INSERT INTO oauth_states VALUES(?1,?2,?3,?4)",params![hash,encrypted,user,now()+600])?;}
 let mut url=reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();url.query_pairs_mut().extend_pairs([("client_id",s.config.google_id.as_str()),("redirect_uri",&format!("{}/api/auth/google/callback",s.config.origin)),("response_type","code"),("scope","openid email profile https://www.googleapis.com/auth/gmail.modify"),("access_type","offline"),("prompt","consent"),("state",&state),("code_challenge",&digest(&verifier)),("code_challenge_method","S256")]);
 let mut response=Json(json!({"url":url.as_str()})).into_response();response.headers_mut().append(header::SET_COOKIE,set_cookie(&s,"tq_oauth",&state,600));Ok(response)
}
#[derive(Deserialize)]struct Callback{code:Option<String>,state:Option<String>,error:Option<String>}
async fn google_callback(State(s):State<AppState>,h:HeaderMap,Query(q):Query<Callback>)->Response {
 match finish_google(&s,&h,q).await{Ok(mut r)=>{*r.status_mut()=StatusCode::SEE_OTHER;r.headers_mut().insert(header::LOCATION,HeaderValue::from_static("/"));r.headers_mut().append(header::SET_COOKIE,set_cookie(&s,"tq_oauth","",0));r},Err(e)=>{let mut url=reqwest::Url::parse(&s.config.origin).expect("validated origin");url.query_pairs_mut().append_pair("auth_error",&e.1);let mut r=Redirect::to(url.as_str()).into_response();r.headers_mut().append(header::SET_COOKIE,set_cookie(&s,"tq_oauth","",0));r}}
}
async fn finish_google(s:&AppState,h:&HeaderMap,q:Callback)->Result<Response>{
 if q.error.is_some(){return Err(bad("Google authorization was cancelled. You can try again."));}let state=q.state.ok_or_else(||bad("Missing OAuth state."))?;if cookie(h,"tq_oauth").as_deref()!=Some(state.as_str()){return Err(bad("Sign-in verification expired. Please start again."));}let hash=digest(&state);
 let (encrypted,link_user):(String,Option<String>)={let mut db=s.db()?;let tx=db.transaction()?;let row=tx.query_row("SELECT verifier,user_id FROM oauth_states WHERE state_hash=?1 AND expires>?2",params![hash,now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||bad("Sign-in expired. Please start again."))?;tx.execute("DELETE FROM oauth_states WHERE state_hash=?1",[&hash])?;tx.commit()?;row};
 if let Some(id)=&link_user{if current(s,h)?.id!=*id{return Err(bad("Your session changed. Please reconnect."));}}
 let verifier=s.unseal(&hash,&encrypted)?;let code=q.code.ok_or_else(||bad("Missing authorization code."))?;
 let response=s.client.post("https://oauth2.googleapis.com/token").form(&[("client_id",s.config.google_id.as_str()),("client_secret",s.config.google_secret.as_str()),("code",&code),("redirect_uri",&format!("{}/api/auth/google/callback",s.config.origin)),("grant_type","authorization_code"),("code_verifier",&verifier)]).send().await?;
 if !response.status().is_success(){return Err(bad("Google sign-in failed. Please try again."));}let tokens:Value=response.json().await?;let access=tokens["access_token"].as_str().ok_or_else(||bad("Google did not return access."))?;
 if !tokens["scope"].as_str().unwrap_or_default().split_whitespace().any(|scope|scope=="https://www.googleapis.com/auth/gmail.modify"){return Err(bad("Gmail permission is needed for mail quests. Please grant it on the consent screen."));}
 let response=s.client.get("https://openidconnect.googleapis.com/v1/userinfo").bearer_auth(access).send().await?;if !response.status().is_success(){return Err(bad("Could not verify your Google identity."));}let profile:Value=response.json().await?;
 if profile["email_verified"]!=true{return Err(bad("Use a verified Google email address."));}let sub=profile["sub"].as_str().ok_or_else(||bad("Missing Google identity."))?;let email=normalize_email(profile["email"].as_str().unwrap_or_default())?;
 let id={let db=s.db()?;let existing:Option<String>=db.query_row("SELECT id FROM users WHERE google_sub=?1",[sub],|r|r.get(0)).optional()?;if let Some(link)=link_user{if existing.as_ref().is_some_and(|v|v!=&link){return Err(bad("This Google account already belongs to another resident."));}db.execute("UPDATE users SET google_sub=?1,terms=?2 WHERE id=?3",params![sub,now(),link])?;link}else if let Some(id)=existing{id}else{let duplicate:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE email=?1)",[&email],|r|r.get(0))?;if duplicate{return Err(bad("An email account already exists. Sign in with its password, then connect Google in Settings."));}let id=uuid::Uuid::new_v4().to_string();db.execute("INSERT INTO users(id,email,name,google_sub,terms) VALUES(?1,?2,?3,?4,?5)",params![id,email,profile["name"].as_str().unwrap_or("Neighbour"),sub,now()])?;id}};
 activation::challenge(s,&id,&email,Some(activation::GoogleCredentials{access:access.into(),refresh:tokens["refresh_token"].as_str().map(String::from),expires:now()+tokens["expires_in"].as_i64().unwrap_or(3600)})).await
}
async fn access_token(s:&AppState,id:&str)->Result<String>{
 let (generation,_)=activation::active(s,id)?;
 let (access,refresh,expires):(String,Option<String>,i64)=s.db()?.query_row("SELECT access,refresh,expires FROM google_tokens WHERE user_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||bad("Connect Gmail in Settings first."))?;
 if expires>now()+60{return s.unseal(id,&access);}let refresh=s.unseal(id,&refresh.ok_or_else(||bad("Your Google permission expired. Please reconnect Gmail."))?)?;
 let response=s.client.post("https://oauth2.googleapis.com/token").form(&[("client_id",s.config.google_id.as_str()),("client_secret",s.config.google_secret.as_str()),("refresh_token",&refresh),("grant_type","refresh_token")]).send().await?;if !response.status().is_success(){return Err(bad("Google access expired or was revoked. Please disconnect and reconnect Gmail."));}let body:Value=response.json().await?;let token=body["access_token"].as_str().ok_or_else(||bad("Please reconnect Gmail."))?;activation::still_active(s,id,&generation)?;let changed=s.db()?.execute("UPDATE google_tokens SET access=?1,expires=?2 WHERE user_id=?3 AND EXISTS(SELECT 1 FROM activations WHERE user_id=?3 AND generation=?4 AND expires>?5)",params![s.seal(id,token)?,now()+body["expires_in"].as_i64().unwrap_or(3600),id,generation,now()])?;if changed==0{return Err(unauthorized());}Ok(token.into())
}
async fn revoke(s:&AppState,id:&str){let token=s.db().ok().and_then(|db|db.query_row("SELECT COALESCE(refresh,access) FROM google_tokens WHERE user_id=?1",[id],|r|r.get::<_,String>(0)).ok()).and_then(|value|s.unseal(id,&value).ok());if let Some(token)=token{let _=s.client.post("https://oauth2.googleapis.com/revoke").form(&[("token",token)]).send().await;}}
async fn disconnect(State(s):State<AppState>,h:HeaderMap)->Result<Json<Value>>{let _guard=s.mail_lock.lock().await;let u=current(&s,&h)?;revoke(&s,&u.id).await;let mut db=s.db()?;let tx=db.transaction()?;tx.execute("DELETE FROM google_tokens WHERE user_id=?1",[&u.id])?;tx.execute("DELETE FROM inbox_cache WHERE user_id=?1",[&u.id])?;tx.execute("DELETE FROM email_challenges WHERE user_id=?1",[&u.id])?;tx.execute("DELETE FROM oauth_states WHERE user_id=?1",[&u.id])?;tx.commit()?;Ok(Json(json!({"ok":true})))}

const GMAIL:&str="https://gmail.googleapis.com/gmail/v1/users/me";
async fn google_json(request:reqwest::RequestBuilder)->Result<Value>{let response=request.send().await?;if !response.status().is_success(){let status=response.status();return Err(AppError(StatusCode::BAD_GATEWAY,if status.as_u16()==401||status.as_u16()==403{"Gmail access was denied. Check permissions and reconnect in Settings."}else if status.as_u16()==429{"Gmail is busy. Wait a moment before trying again."}else{"Gmail did not complete the request. Refresh your inbox before retrying."}.into()));}Ok(response.json().await?)}
fn mail_header(message:&Value,name:&str)->String{message["payload"]["headers"].as_array().and_then(|headers|headers.iter().find(|h|h["name"].as_str().unwrap_or_default().eq_ignore_ascii_case(name))).and_then(|h|h["value"].as_str()).unwrap_or_default().to_string()}
fn find_body(part:&Value,mime:&str)->Option<String>{if part["mimeType"]==mime{if let Some(data)=part["body"]["data"].as_str(){let bytes=URL_SAFE_NO_PAD.decode(data.trim_end_matches('=')).ok()?;return Some(String::from_utf8_lossy(&bytes).into_owned());}}part["parts"].as_array()?.iter().find_map(|p|find_body(p,mime))}
fn has_label(m:&Value,label:&str)->bool{m["labelIds"].as_array().is_some_and(|a|a.iter().any(|v|v==label))}
fn present_mail(m:&Value)->Value{let body=find_body(&m["payload"],"text/plain").or_else(||find_body(&m["payload"],"text/html").map(|html|html2text::from_read(html.as_bytes(),90).unwrap_or_else(|_|"Open this message in Gmail to view its content.".into()))).unwrap_or_else(||m["snippet"].as_str().unwrap_or_default().into());json!({"id":m["id"],"threadId":m["threadId"],"from":mail_header(m,"From"),"subject":mail_header(m,"Subject"),"body":body.chars().take(60000).collect::<String>(),"date":mail_header(m,"Date").chars().take(25).collect::<String>(),"unread":has_label(m,"UNREAD"),"community":!mail_header(m,"List-Id").is_empty()||has_label(m,"CATEGORY_FORUMS")||has_label(m,"CATEGORY_SOCIAL")})}
#[derive(Deserialize)]struct InboxQuery{cursor:Option<String>,#[serde(default)]refresh:bool}
async fn inbox(State(s):State<AppState>,h:HeaderMap,Query(q):Query<InboxQuery>)->Result<Json<Value>>{
 let user=resident(&s,&h)?;let (generation,expires)=activation::active(&s,&user.id)?;
 if q.cursor.is_none()&&!q.refresh {
  let cached:Option<String>=s.db()?.query_row("SELECT payload FROM inbox_cache WHERE user_id=?1 AND generation=?2",params![user.id,generation],|r|r.get(0)).optional()?;
  if let Some(cached)=cached{activation::still_active(&s,&user.id,&generation)?;let data=serde_json::from_str(&s.unseal(&user.id,&cached)?).map_err(|_|bad("Cached inbox is unavailable. Use Refresh."))?;return Ok(Json(data));}
 }
 tokio::time::timeout(Duration::from_secs((expires-now()).max(0) as u64),async {
  let _guard=s.mail_lock.lock().await;resident(&s,&h)?;activation::still_active(&s,&user.id,&generation)?;
  let page=fetch_inbox(&s,&user.id,q.cursor.as_deref()).await?;
  if q.cursor.is_none(){activation::store_inbox(&s,&user.id,&generation,&page)?;}
  activation::still_active(&s,&user.id,&generation)?;Ok(Json(page))
 }).await.map_err(|_|unauthorized())?
}
async fn fetch_inbox(s:&AppState,user_id:&str,cursor:Option<&str>)->Result<Value>{
 let base=s.gmail_base();
 let (generation,_)=activation::active(s,user_id)?;let access=access_token(s,user_id).await?;let mut params=vec![("maxResults","40"),("q","in:inbox")];
 if let Some(cursor)=cursor{if cursor.len()>2048{return Err(bad("Invalid inbox cursor."));}params.push(("pageToken",cursor));}
 activation::still_active(s,user_id,&generation)?;
 let list=google_json(s.client.get(format!("{base}/messages")).bearer_auth(&access).query(&params)).await?;let mut output=Vec::new();
 if let Some(messages)=list["messages"].as_array(){for batch in messages.chunks(8){activation::still_active(s,user_id,&generation)?;let mut jobs=tokio::task::JoinSet::new();for (index,item) in batch.iter().enumerate(){if let Some(id)=item["id"].as_str(){let client=s.client.clone();let token=access.clone();let url=format!("{base}/messages/{id}");jobs.spawn(async move{let message=google_json(client.get(url).bearer_auth(&token).query(&[("format","full")])).await?;Ok::<_,AppError>((index,present_mail(&message)))});}}let mut ordered=Vec::new();while let Some(result)=jobs.join_next().await{ordered.push(result.map_err(|_|AppError(StatusCode::BAD_GATEWAY,"Could not finish loading this inbox page.".into()))??);}ordered.sort_by_key(|(index,_)|*index);output.extend(ordered.into_iter().map(|(_,message)|message));}}
 activation::still_active(s,user_id,&generation)?;Ok(json!({"messages":output,"nextPageToken":list["nextPageToken"].as_str()}))
}
fn safe_header(s:&str)->String{s.chars().filter(|c|!c.is_control()).collect::<String>()}
fn reply_mime(original:&Value,body:&str)->Result<String>{
 let reply_to=mail_header(original,"Reply-To");let recipient=if reply_to.is_empty(){mail_header(original,"From")}else{reply_to};
 if recipient.contains(['\r','\n'])||!recipient.contains('@'){return Err(bad("The sender has no safe reply address. Open this message in Gmail."));}
 let subject=mail_header(original,"Subject");let subject=if subject.to_lowercase().starts_with("re:"){subject}else{format!("Re: {subject}")};let subject=format!("=?UTF-8?B?{}?=",STANDARD.encode(safe_header(&subject)));
 let message_id=safe_header(&mail_header(original,"Message-ID"));let refs=safe_header(&mail_header(original,"References"));let mut headers=format!("To: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n",safe_header(&recipient),subject);
 if !message_id.is_empty(){headers.push_str(&format!("In-Reply-To: {message_id}\r\nReferences: {} {message_id}\r\n",refs));}
 let encoded=STANDARD.encode(body.replace("\r\n","\n").replace('\n',"\r\n"));headers.push_str("\r\n");for chunk in encoded.as_bytes().chunks(76){headers.push_str(std::str::from_utf8(chunk).unwrap());headers.push_str("\r\n");}Ok(URL_SAFE_NO_PAD.encode(headers))
}
#[derive(Deserialize,Default)]struct MailAction{#[serde(default)]body:String}
async fn act(State(s):State<AppState>,h:HeaderMap,Path((id,action)):Path<(String,String)>,Json(payload):Json<MailAction>)->Result<Json<Value>>{
 let user=resident(&s,&h)?;let (generation,expires)=activation::active(&s,&user.id)?;
 tokio::time::timeout(Duration::from_secs((expires-now()).max(0) as u64),act_inner(&s,&h,id,action,payload,&generation)).await.map_err(|_|AppError(StatusCode::UNAUTHORIZED,"Activation expired. If an action was already submitted, check Gmail before retrying.".into()))?
}
async fn act_inner(s:&AppState,h:&HeaderMap,id:String,action:String,payload:MailAction,generation:&str)->Result<Json<Value>>{
 let base=s.gmail_base();
 let user=resident(&s,&h)?;if !matches!(action.as_str(),"reply"|"read"|"archive"){return Err(bad("Unknown mail action."));}if id.len()>128||!id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'||c=='_'){return Err(bad("Invalid message."));}if action=="reply"&&(payload.body.trim().is_empty()||payload.body.len()>80000){return Err(bad("Write a reply of at most 20,000 characters."));}
 // Serialize external mutations. A durable reservation prevents accidental double sends on retries.
 let _guard=s.mail_lock.lock().await;
 resident(s,h)?;activation::still_active(s,&user.id,generation)?;
 let existing:Option<String>=s.db()?.query_row("SELECT status FROM actions WHERE user_id=?1 AND message_id=?2 AND action=?3",params![user.id,id,action],|r|r.get(0)).optional()?;
 if let Some(status)=existing{if status=="done"{return Ok(Json(json!({"ok":true,"xp":user_by_id(&s,&user.id)?.xp,"alreadyCompleted":true})));}return Err(AppError(StatusCode::CONFLICT,"This action has an uncertain delivery status. Check Gmail before sending anything again; it has not been retried.".into()));}
 let token=access_token(&s,&user.id).await?;let original=google_json(s.client.get(format!("{base}/messages/{id}")).bearer_auth(&token).query(&[("format","full")])).await?;
 if action=="read"&&!has_label(&original,"UNREAD")||action=="archive"&&!has_label(&original,"INBOX"){return Ok(Json(json!({"ok":true,"xp":user.xp,"alreadyCompleted":true})));}
 let request=if action=="reply"{let raw=reply_mime(&original,&payload.body)?;s.client.post(format!("{base}/messages/send")).bearer_auth(&token).json(&json!({"raw":raw,"threadId":original["threadId"]}))}else{s.client.post(format!("{base}/messages/{id}/modify")).bearer_auth(&token).json(&json!({"removeLabelIds":[if action=="archive"{"INBOX"}else{"UNREAD"}]}))};
 activation::still_active(s,&user.id,generation)?;
 s.db()?.execute("INSERT INTO actions VALUES(?1,?2,?3,'pending',?4)",params![user.id,id,action,now()])?;
 let response=request.send().await.map_err(|_|AppError(StatusCode::BAD_GATEWAY,"Delivery could not be confirmed. Check Gmail before retrying; Taskocalypse will not automatically resend.".into()))?;
 if !response.status().is_success(){if response.status().is_client_error(){s.db()?.execute("DELETE FROM actions WHERE user_id=?1 AND message_id=?2 AND action=?3",params![user.id,id,action])?;}return Err(AppError(StatusCode::BAD_GATEWAY,"Gmail did not confirm the action. Check Gmail and your connection before retrying.".into()));}
 let points=if action=="reply"{40}else{20};{let mut db=s.db()?;let tx=db.transaction()?;tx.execute("UPDATE actions SET status='done' WHERE user_id=?1 AND message_id=?2 AND action=?3",params![user.id,id,action])?;tx.execute("UPDATE users SET xp=xp+?1,completed=completed+1 WHERE id=?2",params![points,user.id])?;tx.commit()?;}
 s.db()?.execute("DELETE FROM inbox_cache WHERE user_id=?1",[&user.id])?;
 s.db()?.execute("UPDATE activations SET next_sync=0 WHERE user_id=?1 AND generation=?2",params![user.id,generation])?;
 Ok(Json(json!({"ok":true,"xp":user_by_id(&s,&user.id)?.xp})))
}

fn router(state:AppState,dist:&str)->Router{Router::new()
 .route("/api/health",get(||async{Json(json!({"ok":true,"service":"taskocalypse-rust"}))}))
 .route("/api/auth/register",post(register)).route("/api/auth/login",post(login)).route("/api/auth/me",get(me))
 .route("/api/auth/challenge",get(activation::challenge_status)).route("/api/auth/verify",post(activation::verify)).route("/api/auth/resend",post(activation::resend))
 .route("/api/auth/logout",post(logout)).route("/api/auth/terms",post(terms)).route("/api/auth/account",delete(remove_account))
 .route("/api/auth/google/start",post(google_start)).route("/api/auth/google/callback",get(google_callback)).route("/api/auth/google/disconnect",post(disconnect))
 .route("/api/mail",get(inbox)).route("/api/mail/{id}/{action}",post(act))
 .route("/api/{*path}",get(||async{AppError(StatusCode::NOT_FOUND,"Unknown API route.".into())}).post(||async{AppError(StatusCode::NOT_FOUND,"Unknown API route.".into())}))
 .fallback_service(ServeDir::new(dist).not_found_service(ServeFile::new(format!("{dist}/index.html"))))
 .layer(DefaultBodyLimit::max(128*1024)).layer(middleware::from_fn_with_state(state.clone(),security)).with_state(state)}

#[tokio::main]
async fn main()->std::result::Result<(),Box<dyn std::error::Error>>{
 dotenvy::dotenv().ok();tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
 let origin=std::env::var("APP_URL").unwrap_or_else(|_|"http://localhost:8080".into()).trim_end_matches('/').to_string();let parsed=reqwest::Url::parse(&origin)?;if !["http","https"].contains(&parsed.scheme())||parsed.path()!="/"||parsed.query().is_some()||parsed.fragment().is_some(){return Err("APP_URL must be an HTTP(S) origin".into());}
 let key=STANDARD.decode(std::env::var("TOKEN_ENCRYPTION_KEY").map_err(|_|"Set TOKEN_ENCRYPTION_KEY to a base64-encoded 32-byte random value. See SETUP.md.")?)?;if key.len()!=32{return Err("TOKEN_ENCRYPTION_KEY must decode to 32 bytes".into());}
 let db=Connection::open(std::env::var("DATABASE_PATH").unwrap_or_else(|_|"taskocalypse.db".into()))?;db.busy_timeout(Duration::from_secs(5))?;initialize(&db)?;
 let otp_api_url=std::env::var("OTP_API_URL").unwrap_or_else(|_|"https://api.resend.com/emails".into());
 let endpoint=reqwest::Url::parse(&otp_api_url)?;
 let local=|url:&reqwest::Url|matches!(url.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"));
 if otp_api_url!="https://api.resend.com/emails" && !(local(&parsed)&&local(&endpoint)&&endpoint.scheme()=="http") {return Err("OTP_API_URL override is allowed only for loopback integration tests with a loopback APP_URL".into());}
 if parsed.scheme()!="https"&&!local(&parsed){return Err("A deployed APP_URL must use HTTPS".into());}
 let state=AppState{db:Arc::new(Mutex::new(db)),client:reqwest::Client::builder().timeout(Duration::from_secs(25)).redirect(reqwest::redirect::Policy::none()).build()?,config:Arc::new(Config{secure:origin.starts_with("https://"),origin,google_id:std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),google_secret:std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default(),otp_api_key:std::env::var("OTP_API_KEY").unwrap_or_default(),otp_from:std::env::var("OTP_FROM").unwrap_or_default(),otp_api_url}),cipher:Arc::new(ChaCha20Poly1305::new_from_slice(&key).expect("validated key")),otp_key:Sha256::digest([b"taskocalypse-otp-key-v1".as_slice(),&key].concat()).into(),limits:Default::default(),mail_lock:Default::default(),#[cfg(test)]test_outbox:Default::default(),#[cfg(test)]test_gmail_base:None};
 activation::cleanup(&state).map_err(|_|"Could not clean up expired activations at startup")?;
 activation::start_workers(state.clone());
 let address=std::env::var("BIND_ADDRESS").unwrap_or_else(|_|"127.0.0.1:8080".into());let dist=std::env::var("STATIC_DIR").unwrap_or_else(|_|"dist".into());let listener=tokio::net::TcpListener::bind(&address).await?;
 println!("Taskocalypse Rust server listening on {}",state.config.origin);
 axum::serve(listener,router(state,&dist).into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async{let _=tokio::signal::ctrl_c().await;}).await?;Ok(())
}

#[cfg(test)]
mod tests;
