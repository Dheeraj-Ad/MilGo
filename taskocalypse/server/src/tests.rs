use super::*;
use tower::ServiceExt;
use axum::body::Body;
use http_body_util::BodyExt;

fn state()->AppState {
    let db=Connection::open_in_memory().unwrap();initialize(&db).unwrap();
    AppState{db:Arc::new(Mutex::new(db)),client:reqwest::Client::new(),
        config:Arc::new(Config{origin:"http://localhost:8080".into(),google_id:String::new(),google_secret:String::new(),secure:false,
            otp_api_key:"test-key".into(),otp_from:"Town <town@example.com>".into(),otp_api_url:"test-outbox".into()}),
        cipher:Arc::new(ChaCha20Poly1305::new_from_slice(&[7;32]).unwrap()),otp_key:[8;32],
        limits:Default::default(),mail_lock:Default::default(),test_outbox:Default::default(),test_gmail_base:None}
}
fn request(path:&str,body:Value,cookie:Option<&str>)->Request {
    let mut r=Request::builder().method("POST").uri(path).header("content-type","application/json")
        .header("x-taskocalypse","town").header("origin","http://localhost:8080");
    if let Some(cookie)=cookie{r=r.header("cookie",cookie);}
    r.body(Body::from(body.to_string())).unwrap()
}
fn read(path:&str,cookie:&str)->Request {
    Request::builder().uri(path).header("cookie",cookie).body(Body::empty()).unwrap()
}
fn cookie_value(r:&Response,name:&str)->String {
    r.headers().get_all(header::SET_COOKIE).iter().filter_map(|h|h.to_str().ok())
        .find(|value|value.starts_with(&format!("{name}="))).unwrap().split(';').next().unwrap().into()
}
async fn body(r:Response)->Value {serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap()}
async fn begin(s:&AppState)->(String,String) {
    let r=router(s.clone(),"missing").oneshot(request("/api/auth/register",
        json!({"email":"hello@example.com","name":"Milo","password":"a very good password","terms":true}),None)).await.unwrap();
    assert_eq!(r.status(),StatusCode::OK);let cookie=cookie_value(&r,"tq_pending");
    let data=body(r).await;assert_eq!(data["verificationRequired"],true);assert!(data.get("code").is_none());
    assert!(data.get("password").is_none());assert!(data.get("accessToken").is_none());
    let code=s.test_outbox.lock().unwrap().last().unwrap().1.clone();(cookie,code)
}
async fn activate(s:&AppState)->(String,String) {
    let (pending,code)=begin(s).await;
    let r=router(s.clone(),"missing").oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap();
    assert_eq!(r.status(),StatusCode::OK);let session=cookie_value(&r,"tq_session");let user=body(r).await;
    (session,user["id"].as_str().unwrap().into())
}
fn headers(cookie:&str)->HeaderMap {let mut h=HeaderMap::new();h.insert(header::COOKIE,HeaderValue::from_str(cookie).unwrap());h}
fn count(s:&AppState,table:&str)->i64{s.db().unwrap().query_row(&format!("SELECT count(*) FROM {table}"),[],|r|r.get(0)).unwrap()}
fn add_google(s:&AppState,user:&str){s.db().unwrap().execute("INSERT INTO google_tokens VALUES(?1,?2,?3,?4)",params![user,s.seal(user,"access-secret").unwrap(),s.seal(user,"refresh-secret").unwrap(),now()+3600]).unwrap();}

#[tokio::test]
async fn password_alone_never_grants_session_or_gmail_access(){
    let s=state();let (pending,_)=begin(&s).await;let app=router(s.clone(),"missing");
    assert_eq!(count(&s,"sessions"),0);assert_eq!(count(&s,"activations"),0);
    for path in ["/api/auth/me","/api/mail"]{assert_eq!(app.clone().oneshot(read(path,&pending)).await.unwrap().status(),StatusCode::UNAUTHORIZED);}
    assert_eq!(app.oneshot(read("/api/auth/challenge",&pending)).await.unwrap().status(),StatusCode::OK);
}

#[tokio::test]
async fn otp_is_browser_bound_single_use_and_requires_terms(){
    let s=state();let (pending,code)=begin(&s).await;let app=router(s.clone(),"missing");
    assert_eq!(app.clone().oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some("tq_pending=another-browser"))).await.unwrap().status(),StatusCode::BAD_REQUEST);
    assert_eq!(app.clone().oneshot(request("/api/auth/verify",json!({"code":code,"terms":false}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);
    let r=app.clone().oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap();
    assert_eq!(r.status(),StatusCode::OK);let session=cookie_value(&r,"tq_session");let u=body(r).await;
    assert!((10798..=10800).contains(&(u["expiresAt"].as_i64().unwrap()-now())));
    assert_eq!(app.clone().oneshot(read("/api/auth/me",&session)).await.unwrap().status(),StatusCode::OK);
    assert_eq!(app.oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn simultaneous_code_submission_creates_exactly_one_session(){
    let s=state();let (pending,code)=begin(&s).await;let app=router(s.clone(),"missing");
    let (a,b)=tokio::join!(app.clone().oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))),app.oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))));
    let mut statuses=vec![a.unwrap().status().as_u16(),b.unwrap().status().as_u16()];statuses.sort();assert_eq!(statuses,vec![200,400]);assert_eq!(count(&s,"sessions"),1);
}

#[tokio::test]
async fn code_expires_at_exact_deadline(){
    let s=state();let (pending,code)=begin(&s).await;
    s.db().unwrap().execute("UPDATE email_challenges SET expires=?1",[now()]).unwrap();
    assert_eq!(router(s.clone(),"missing").oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);
    assert_eq!(count(&s,"sessions"),0);
}

#[tokio::test]
async fn five_wrong_codes_invalidate_challenge_and_staged_google_tokens(){
    let s=state();let (pending,correct)=begin(&s).await;
    let hash=digest(pending.strip_prefix("tq_pending=").unwrap());
    s.db().unwrap().execute("INSERT INTO pending_google VALUES(?1,'encrypted-access','encrypted-refresh',?2)",params![hash,now()+3600]).unwrap();
    let wrong=if correct=="000000"{"111111"}else{"000000"};let app=router(s.clone(),"missing");
    for _ in 0..5{assert_eq!(app.clone().oneshot(request("/api/auth/verify",json!({"code":wrong,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);}
    assert_eq!(count(&s,"pending_google"),0);assert_eq!(count(&s,"email_challenges"),0);
    assert_eq!(app.oneshot(request("/api/auth/verify",json!({"code":correct,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn resend_replaces_code_without_resetting_deadline_or_attempts(){
    let s=state();let (pending,old)=begin(&s).await;let app=router(s.clone(),"missing");
    assert_eq!(app.clone().oneshot(request("/api/auth/resend",json!({}),Some(&pending))).await.unwrap().status(),StatusCode::TOO_MANY_REQUESTS);
    s.db().unwrap().execute("UPDATE otp_limits SET last_send=?1",[now()-61]).unwrap();
    s.db().unwrap().execute("UPDATE email_challenges SET attempts=2",[]).unwrap();
    let before:i64=s.db().unwrap().query_row("SELECT expires FROM email_challenges",[],|r|r.get(0)).unwrap();
    assert_eq!(app.clone().oneshot(request("/api/auth/resend",json!({}),Some(&pending))).await.unwrap().status(),StatusCode::OK);
    let (expires,attempts):(i64,i64)=s.db().unwrap().query_row("SELECT expires,attempts FROM email_challenges",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert_eq!((expires,attempts),(before,2));
    let new=s.test_outbox.lock().unwrap().last().unwrap().1.clone();
    if new!=old{assert_eq!(app.clone().oneshot(request("/api/auth/verify",json!({"code":old,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::BAD_REQUEST);}
    assert_eq!(app.oneshot(request("/api/auth/verify",json!({"code":new,"terms":true}),Some(&pending))).await.unwrap().status(),StatusCode::OK);
}

#[tokio::test]
async fn second_signin_does_not_extend_activation(){
    let s=state();let (_,user)=activate(&s).await;
    let expiry=now()+300;s.db().unwrap().execute("UPDATE activations SET expires=?1 WHERE user_id=?2",params![expiry,user]).unwrap();
    s.db().unwrap().execute("UPDATE otp_limits SET last_send=?1",[now()-61]).unwrap();
    let app=router(s.clone(),"missing");let r=app.clone().oneshot(request("/api/auth/login",json!({"email":"hello@example.com","password":"a very good password","terms":true}),None)).await.unwrap();assert_eq!(r.status(),StatusCode::OK);
    let pending=cookie_value(&r,"tq_pending");let code=s.test_outbox.lock().unwrap().last().unwrap().1.clone();
    let r=app.oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap();assert_eq!(r.status(),StatusCode::OK);assert_eq!(body(r).await["expiresAt"],expiry);
}

#[tokio::test]
async fn expiry_blocks_credentials_before_cleanup_and_survives_restart(){
    let s=state();let (cookie,user)=activate(&s).await;add_google(&s,&user);
    let (generation,_)=activation::active(&s,&user).unwrap();activation::store_inbox(&s,&user,&generation,&json!({"messages":[]})).unwrap();
    s.db().unwrap().execute("UPDATE activations SET expires=?1",[now()]).unwrap();
    assert!(current(&s,&headers(&cookie)).is_err());assert!(access_token(&s,&user).await.is_err());
    assert_eq!(router(s.clone(),"missing").oneshot(request("/api/mail/abc/reply",json!({"body":"do not send"}),Some(&cookie))).await.unwrap().status(),StatusCode::UNAUTHORIZED);
    initialize(&s.db().unwrap()).unwrap();activation::cleanup(&s).unwrap();
    for table in ["sessions","activations","google_tokens","inbox_cache"]{assert_eq!(count(&s,table),0);}
    assert_eq!(count(&s,"users"),1);
}

#[tokio::test]
async fn logout_ends_activation_and_stale_sync_cannot_restore_cache(){
    let s=state();let (cookie,user)=activate(&s).await;add_google(&s,&user);let (generation,_)=activation::active(&s,&user).unwrap();
    let r=router(s.clone(),"missing").oneshot(request("/api/auth/logout",json!({}),Some(&cookie))).await.unwrap();assert_eq!(r.status(),StatusCode::OK);
    assert!(activation::store_inbox(&s,&user,&generation,&json!({"messages":[]})).is_err());
    for table in ["sessions","activations","google_tokens","inbox_cache"]{assert_eq!(count(&s,table),0);}
}

#[tokio::test]
async fn queued_mail_action_rechecks_expiry_after_lock(){
    let s=state();let (cookie,_)=activate(&s).await;let guard=s.mail_lock.lock().await;
    let app=router(s.clone(),"missing");let task=tokio::spawn(async move{app.oneshot(request("/api/mail/abc/archive",json!({}),Some(&cookie))).await.unwrap()});
    tokio::task::yield_now().await;s.db().unwrap().execute("UPDATE activations SET expires=?1",[now()]).unwrap();drop(guard);
    assert_eq!(task.await.unwrap().status(),StatusCode::UNAUTHORIZED);assert_eq!(count(&s,"actions"),0);
}

#[tokio::test]
async fn encrypted_google_credentials_stay_pending_until_email_verification(){
    let s=state();let (_,user)=activate(&s).await;s.db().unwrap().execute("UPDATE otp_limits SET last_send=?1",[now()-61]).unwrap();
    let r=activation::challenge(&s,&user,"hello@example.com",Some(activation::GoogleCredentials{access:"new-access".into(),refresh:Some("new-refresh".into()),expires:now()+3600})).await.unwrap();
    let pending=cookie_value(&r,"tq_pending");assert_eq!(count(&s,"google_tokens"),0);assert_eq!(count(&s,"pending_google"),1);
    let encrypted:String=s.db().unwrap().query_row("SELECT access FROM pending_google",[],|r|r.get(0)).unwrap();assert_ne!(encrypted,"new-access");
    let code=s.test_outbox.lock().unwrap().last().unwrap().1.clone();
    let r=router(s.clone(),"missing").oneshot(request("/api/auth/verify",json!({"code":code,"terms":true}),Some(&pending))).await.unwrap();assert_eq!(r.status(),StatusCode::OK);
    assert_eq!(count(&s,"pending_google"),0);assert_eq!(access_token(&s,&user).await.unwrap(),"new-access");
}

#[tokio::test]
async fn background_sync_reads_only_and_stops_at_expiry(){
    use std::sync::atomic::{AtomicUsize,Ordering};
    let reads=Arc::new(AtomicUsize::new(0));let counter=reads.clone();
    let mock=Router::new().route("/messages",get(move||{let counter=counter.clone();async move{counter.fetch_add(1,Ordering::SeqCst);Json(json!({"messages":[{"id":"message1"}]}))}}))
        .route("/messages/message1",get(||async{Json(json!({"id":"message1","threadId":"thread1","labelIds":["UNREAD"],"payload":{"mimeType":"text/plain","headers":[],"body":{"data":URL_SAFE_NO_PAD.encode("hello from mock")}}}))}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();let server=tokio::spawn(async move{axum::serve(listener,mock).await.unwrap()});
    let mut s=state();s.test_gmail_base=Some(format!("http://{addr}"));let (_,user)=activate(&s).await;add_google(&s,&user);
    activation::sync_due(&s).await.unwrap();assert_eq!(reads.load(Ordering::SeqCst),1);assert_eq!(count(&s,"actions"),0);
    let encrypted:String=s.db().unwrap().query_row("SELECT payload FROM inbox_cache",[],|r|r.get(0)).unwrap();assert!(!encrypted.contains("hello from mock"));assert!(s.unseal(&user,&encrypted).unwrap().contains("hello from mock"));
    s.db().unwrap().execute("UPDATE activations SET expires=?1,next_sync=0",[now()]).unwrap();activation::sync_due(&s).await.unwrap();assert_eq!(reads.load(Ordering::SeqCst),1);
    server.abort();
}

#[test]
fn migration_invalidates_old_sessions_without_deleting_accounts(){
    let db=Connection::open_in_memory().unwrap();db.execute_batch("CREATE TABLE users(id TEXT PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL,password TEXT,google_sub TEXT UNIQUE,terms INTEGER NOT NULL DEFAULT 0,xp INTEGER NOT NULL DEFAULT 0,completed INTEGER NOT NULL DEFAULT 0); CREATE TABLE sessions(token_hash TEXT PRIMARY KEY,user_id TEXT,expires INTEGER); CREATE TABLE google_tokens(user_id TEXT PRIMARY KEY,access TEXT,refresh TEXT,expires INTEGER); INSERT INTO users(id,email,name) VALUES('old','old@example.com','Old'); INSERT INTO sessions VALUES('old-token','old',9999999999); INSERT INTO google_tokens VALUES('old','access','refresh',9999999999);").unwrap();initialize(&db).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM sessions",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT count(*) FROM google_tokens",[],|r|r.get::<_,i64>(0)).unwrap(),0);assert_eq!(db.query_row("SELECT count(*) FROM users",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[tokio::test]
async fn missing_mailer_fails_closed(){let mut s=state();Arc::get_mut(&mut s.config).unwrap().otp_api_key.clear();let r=router(s.clone(),"missing").oneshot(request("/api/auth/register",json!({"email":"x@example.com","name":"Milo","password":"a very good password","terms":true}),None)).await.unwrap();assert_eq!(r.status(),StatusCode::SERVICE_UNAVAILABLE);assert_eq!(count(&s,"sessions"),0);assert_eq!(count(&s,"users"),0);}

#[tokio::test]
async fn rejects_unconsented_account_and_cross_origin(){let app=router(state(),"missing");assert_eq!(app.clone().oneshot(request("/api/auth/register",json!({"email":"x@example.com","name":"Milo","password":"a very good password","terms":false}),None)).await.unwrap().status(),StatusCode::BAD_REQUEST);let mut req=request("/api/auth/login",json!({"email":"x@example.com","password":"anything"}),None);req.headers_mut().insert(header::ORIGIN,HeaderValue::from_static("https://evil.example"));assert_eq!(app.clone().oneshot(req).await.unwrap().status(),StatusCode::FORBIDDEN);assert_eq!(app.oneshot(Request::builder().method("POST").uri("/api/auth/logout").body(Body::empty()).unwrap()).await.unwrap().status(),StatusCode::FORBIDDEN);}

#[test]fn token_encryption_binds_to_user(){let s=state();let encrypted=s.seal("alice","secret-refresh-token").unwrap();assert_ne!(encrypted,"secret-refresh-token");assert_eq!(s.unseal("alice",&encrypted).unwrap(),"secret-refresh-token");assert!(s.unseal("bob",&encrypted).is_err());}
#[test]fn mime_rejects_header_injection(){let mut m=json!({"payload":{"headers":[{"name":"From","value":"Milo <milo@example.com>"},{"name":"Subject","value":"Hello"},{"name":"Message-ID","value":"<123@example.com>"}]}});let raw=reply_mime(&m,"Hello, neighbour!").unwrap();let decoded=String::from_utf8(URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();assert!(decoded.contains("In-Reply-To: <123@example.com>"));m["payload"]["headers"][0]["value"]=json!("milo@example.com\r\nBcc: thief@example.com");assert!(reply_mime(&m,"hello").is_err());}
#[test]fn multipart_mail_is_plain_text_and_community_detected(){let m=json!({"id":"1","threadId":"2","labelIds":["UNREAD","CATEGORY_FORUMS"],"payload":{"mimeType":"multipart/alternative","headers":[],"parts":[{"mimeType":"text/plain","body":{"data":URL_SAFE_NO_PAD.encode("Meet at the square")}}]}});let v=present_mail(&m);assert_eq!(v["body"],"Meet at the square");assert_eq!(v["community"],true);}
