//! Email proof, fixed-lifetime activations, and server-owned inbox synchronization.
use super::*;
use hmac::{Hmac, Mac};
use rand::Rng;

pub const LIFETIME: i64 = 3 * 60 * 60;
const OTP_LIFETIME: i64 = 10 * 60;
const RESEND_DELAY: i64 = 60;
const TERMS_VERSION: &str = "2026-10-31";

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let has_generation = db.prepare("PRAGMA table_info(sessions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?.contains(&"activation_id".to_string());
    if !has_generation {
        // Existing twelve-hour sessions never inherit the new, verified privileges.
        db.execute_batch("ALTER TABLE sessions ADD COLUMN activation_id TEXT; DELETE FROM sessions; DELETE FROM google_tokens;")?;
    }
    db.execute_batch("
        CREATE TABLE IF NOT EXISTS activations (
            user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            generation TEXT NOT NULL, started INTEGER NOT NULL, expires INTEGER NOT NULL,
            terms_version TEXT NOT NULL, next_sync INTEGER NOT NULL DEFAULT 0,
            last_synced INTEGER, sync_error TEXT
        );
        CREATE TABLE IF NOT EXISTS email_challenges (
            token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
            email TEXT NOT NULL, code_mac TEXT NOT NULL, expires INTEGER NOT NULL,
            sent_at INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
            ready INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS pending_google (
            challenge_hash TEXT PRIMARY KEY REFERENCES email_challenges(token_hash) ON DELETE CASCADE,
            access TEXT NOT NULL, refresh TEXT, expires INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS otp_limits (
            user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            window_start INTEGER NOT NULL, sends INTEGER NOT NULL, failures INTEGER NOT NULL,
            last_send INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS inbox_cache (
            user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            generation TEXT NOT NULL, payload TEXT NOT NULL, synced_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS activations_expiry ON activations(expires);
        CREATE INDEX IF NOT EXISTS challenges_expiry ON email_challenges(expires);
    ")
}

pub fn ensure_mailer(s: &AppState) -> Result<()> {
    if s.config.otp_api_key.is_empty() || s.config.otp_from.is_empty() {
        return Err(AppError(StatusCode::SERVICE_UNAVAILABLE,
            "Email verification is not configured. Set OTP_API_KEY and OTP_FROM on the server; sign-in stays disabled until delivery is configured.".into()));
    }
    Ok(())
}

fn otp_mac(s: &AppState, challenge: &str, code: &str) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&s.otp_key).expect("fixed-size OTP key");
    mac.update(b"taskocalypse-email-otp-v1\0");
    mac.update(challenge.as_bytes());
    mac.update(b"\0");
    mac.update(code.as_bytes());
    mac
}

fn mask_email(email: &str) -> String {
    let (name, domain) = email.split_once('@').unwrap_or(("", ""));
    format!("{}***@{}", name.chars().next().unwrap_or('*'), domain)
}

fn info(email: &str, expires: i64, sent: i64) -> Value {
    json!({"verificationRequired":true, "email":mask_email(email),
        "expiresAt":expires, "resendAfter":sent + RESEND_DELAY})
}

pub struct GoogleCredentials {
    pub access: String,
    pub refresh: Option<String>,
    pub expires: i64,
}

fn send_budget(db: &Connection, user: &str, time: i64) -> Result<()> {
    db.execute("INSERT INTO otp_limits VALUES(?1,?2,0,0,0) ON CONFLICT(user_id) DO NOTHING", params![user,time])?;
    db.execute("UPDATE otp_limits SET window_start=?1,sends=0,failures=0 WHERE user_id=?2 AND window_start<=?3", params![time,user,time-3600])?;
    let (count, failures, last): (i64,i64,i64) = db.query_row(
        "SELECT sends,failures,last_send FROM otp_limits WHERE user_id=?1", [user],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if count >= 5 || failures >= 15 || last + RESEND_DELAY > time {
        return Err(AppError(StatusCode::TOO_MANY_REQUESTS,
            "Wait at least 60 seconds between codes. At most five codes and fifteen failed guesses are allowed per account per hour.".into()));
    }
    db.execute("UPDATE otp_limits SET sends=sends+1,last_send=?1 WHERE user_id=?2",params![time,user])?;
    Ok(())
}

async fn deliver(s: &AppState, email: &str, code: &str, delivery_id: &str) -> Result<()> {
    #[cfg(test)]
    if s.config.otp_api_url == "test-outbox" {
        s.test_outbox.lock().unwrap().push((email.to_string(),code.to_string()));
        return Ok(());
    }
    let response = s.client.post(&s.config.otp_api_url)
        .bearer_auth(&s.config.otp_api_key)
        .header("Idempotency-Key",delivery_id)
        .json(&json!({"from":s.config.otp_from,"to":[email],
            "subject":"Your Taskocalypse verification code",
            "text":format!("Your Taskocalypse code is: {code}\n\nIt expires in 10 minutes and works only in the browser that requested it. Entering it activates access for up to three hours after you accept the terms. Never share this code. If you did not request it, ignore this email.")}))
        .send().await.map_err(|_| AppError(StatusCode::BAD_GATEWAY,"The verification email could not be confirmed. Please try again after one minute.".into()))?;
    if !response.status().is_success() {
        // Provider bodies can contain personal information: do not log or forward them.
        return Err(AppError(StatusCode::BAD_GATEWAY,"The email provider did not accept the verification email. Check the server's sender configuration.".into()));
    }
    Ok(())
}

pub async fn challenge(s: &AppState, user: &str, email: &str, google: Option<GoogleCredentials>) -> Result<Response> {
    ensure_mailer(s)?;
    let token=random(); let hash=digest(&token); let time=now();
    let code=format!("{:06}",OsRng.gen_range(0..1_000_000u32));
    let mac=URL_SAFE_NO_PAD.encode(otp_mac(s,&hash,&code).finalize().into_bytes());
    {
        let mut db=s.db()?; let tx=db.transaction()?;
        send_budget(&tx,user,time)?;
        tx.execute("DELETE FROM email_challenges WHERE user_id=?1",[user])?;
        tx.execute("INSERT INTO email_challenges(token_hash,user_id,email,code_mac,expires,sent_at) VALUES(?1,?2,?3,?4,?5,?6)",params![hash,user,email,mac,time+OTP_LIFETIME,time])?;
        if let Some(g)=google {
            tx.execute("INSERT INTO pending_google VALUES(?1,?2,?3,?4)",params![hash,s.seal(user,&g.access)?,g.refresh.map(|v|s.seal(user,&v)).transpose()?,g.expires])?;
        }
        tx.commit()?;
    }
    if let Err(error)=deliver(s,email,&code,&hash).await {
        s.db()?.execute("DELETE FROM email_challenges WHERE token_hash=?1",[&hash])?;
        return Err(error);
    }
    s.db()?.execute("UPDATE email_challenges SET ready=1 WHERE token_hash=?1",[&hash])?;
    let mut response=Json(info(email,time+OTP_LIFETIME,time)).into_response();
    response.headers_mut().append(header::SET_COOKIE,set_cookie(s,"tq_pending",&token,OTP_LIFETIME));
    Ok(response)
}

pub async fn challenge_status(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>> {
    let hash=digest(&cookie(&h,"tq_pending").ok_or_else(unauthorized)?);
    let (email,expires,sent):(String,i64,i64)=s.db()?.query_row(
        "SELECT email,expires,sent_at FROM email_challenges WHERE token_hash=?1 AND expires>?2 AND ready=1 AND attempts<5",
        params![hash,now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||bad("Verification expired. Sign in again to request a new code."))?;
    Ok(Json(info(&email,expires,sent)))
}

pub async fn resend(State(s): State<AppState>, h: HeaderMap) -> Result<Json<Value>> {
    ensure_mailer(&s)?;
    let hash=digest(&cookie(&h,"tq_pending").ok_or_else(unauthorized)?);
    let code=format!("{:06}",OsRng.gen_range(0..1_000_000u32)); let time=now();
    let (email,expires)={
        let mut db=s.db()?;let tx=db.transaction()?;
        let (user,email,expires):(String,String,i64)=tx.query_row(
            "SELECT user_id,email,expires FROM email_challenges WHERE token_hash=?1 AND expires>?2 AND attempts<5",
            params![hash,time],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||bad("Verification expired. Sign in again."))?;
        send_budget(&tx,&user,time)?;
        let mac=URL_SAFE_NO_PAD.encode(otp_mac(&s,&hash,&code).finalize().into_bytes());
        // Resends replace the code without resetting its deadline or failed-guess budget.
        tx.execute("UPDATE email_challenges SET code_mac=?1,sent_at=?2,ready=0 WHERE token_hash=?3",params![mac,time,hash])?;
        tx.commit()?;(email,expires)
    };
    deliver(&s,&email,&code,&format!("{hash}-{time}")).await?;
    s.db()?.execute("UPDATE email_challenges SET ready=1 WHERE token_hash=?1 AND sent_at=?2",params![hash,time])?;
    Ok(Json(info(&email,expires,time)))
}

#[derive(Deserialize)]
pub struct Verification { code:String, terms:bool }

pub async fn verify(State(s):State<AppState>,h:HeaderMap,Json(v):Json<Verification>) -> Result<Response> {
    if !v.terms { return Err(bad("Accept the terms to activate your account.")); }
    let hash=digest(&cookie(&h,"tq_pending").ok_or_else(unauthorized)?);
    let code=v.code.trim();
    if code.len()!=6 || !code.bytes().all(|b|b.is_ascii_digit()) {return Err(bad("Enter the six-digit code from your email."));}
    // Shares the operation lock with logout/disconnect and external mutations.
    let _guard=s.mail_lock.lock().await;
    let token=random();let time=now();
    let (user,expires)={
        let mut db=s.db()?;let tx=db.transaction()?;
        let (user,mac,attempts):(String,String,i64)=tx.query_row(
            "SELECT user_id,code_mac,attempts FROM email_challenges WHERE token_hash=?1 AND expires>?2 AND ready=1",
            params![hash,time],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||bad("Verification expired or already used. Sign in again."))?;
        let failures:i64=tx.query_row("SELECT failures FROM otp_limits WHERE user_id=?1",[&user],|r|r.get(0))?;
        if attempts>=5 || failures>=15 {return Err(AppError(StatusCode::TOO_MANY_REQUESTS,"Too many incorrect codes. Sign in again after the verification cooldown.".into()));}
        let expected=URL_SAFE_NO_PAD.decode(mac).map_err(|_|bad("Invalid verification state."))?;
        if otp_mac(&s,&hash,code).verify_slice(&expected).is_err() {
            tx.execute("UPDATE email_challenges SET attempts=attempts+1 WHERE token_hash=?1",[&hash])?;
            tx.execute("UPDATE otp_limits SET failures=failures+1 WHERE user_id=?1",[&user])?;
            if attempts+1>=5 {tx.execute("DELETE FROM email_challenges WHERE token_hash=?1",[&hash])?;}
            tx.commit()?;
            return Err(bad("Incorrect code. Five failed attempts invalidate this verification."));
        }
        let active:Option<(String,i64)>=tx.query_row("SELECT generation,expires FROM activations WHERE user_id=?1 AND expires>?2",params![user,time],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (generation,expires)=if let Some(existing)=active {existing} else {
            tx.execute("DELETE FROM sessions WHERE user_id=?1",[&user])?;
            tx.execute("DELETE FROM google_tokens WHERE user_id=?1",[&user])?;
            tx.execute("DELETE FROM inbox_cache WHERE user_id=?1",[&user])?;
            let generation=random();let expires=time+LIFETIME;
            tx.execute("INSERT INTO activations(user_id,generation,started,expires,terms_version) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(user_id) DO UPDATE SET generation=excluded.generation,started=excluded.started,expires=excluded.expires,terms_version=excluded.terms_version,next_sync=0,last_synced=NULL,sync_error=NULL",params![user,generation,time,expires,TERMS_VERSION])?;
            (generation,expires)
        };
        let pending:Option<(String,Option<String>,i64)>=tx.query_row("SELECT access,refresh,expires FROM pending_google WHERE challenge_hash=?1",[&hash],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((access,refresh,token_expiry))=pending {
            tx.execute("INSERT INTO google_tokens VALUES(?1,?2,?3,?4) ON CONFLICT(user_id) DO UPDATE SET access=excluded.access,refresh=excluded.refresh,expires=excluded.expires",params![user,access,refresh,token_expiry])?;
            tx.execute("DELETE FROM inbox_cache WHERE user_id=?1",[&user])?;
            tx.execute("UPDATE activations SET next_sync=0 WHERE user_id=?1",[&user])?;
        }
        tx.execute("UPDATE users SET terms=?1 WHERE id=?2",params![time,user])?;
        tx.execute("DELETE FROM email_challenges WHERE token_hash=?1",[&hash])?;
        if let Some(old)=cookie(&h,"tq_session"){tx.execute("DELETE FROM sessions WHERE token_hash=?1",[digest(&old)])?;}
        tx.execute("INSERT INTO sessions(token_hash,user_id,expires,activation_id) VALUES(?1,?2,?3,?4)",params![digest(&token),user,expires,generation])?;
        tx.commit()?;(user,expires)
    };
    let mut response=Json(user_by_id(&s,&user)?).into_response();
    response.headers_mut().append(header::SET_COOKIE,set_cookie(&s,"tq_session",&token,expires-now()));
    response.headers_mut().append(header::SET_COOKIE,set_cookie(&s,"tq_pending","",0));
    Ok(response)
}

pub fn active(s:&AppState,user:&str) -> Result<(String,i64)> {
    s.db()?.query_row("SELECT generation,expires FROM activations WHERE user_id=?1 AND expires>?2",params![user,now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||AppError(StatusCode::UNAUTHORIZED,"Your three-hour activation has ended. Sign in and verify your email again.".into()))
}

pub fn still_active(s:&AppState,user:&str,generation:&str) -> Result<()> {
    let (current,_)=active(s,user)?;
    if current!=generation {return Err(unauthorized());}Ok(())
}

pub fn terminate(db:&Connection,user:&str) -> rusqlite::Result<()> {
    db.execute("DELETE FROM sessions WHERE user_id=?1",[user])?;
    db.execute("DELETE FROM google_tokens WHERE user_id=?1",[user])?;
    db.execute("DELETE FROM inbox_cache WHERE user_id=?1",[user])?;
    db.execute("DELETE FROM activations WHERE user_id=?1",[user])?;
    db.execute("DELETE FROM email_challenges WHERE user_id=?1",[user])?;
    db.execute("DELETE FROM oauth_states WHERE user_id=?1",[user])?;
    Ok(())
}

pub fn cleanup(s:&AppState) -> Result<()> {
    let mut db=s.db()?;let tx=db.transaction()?;let time=now();
    // Every API independently checks the deadline. Cleanup additionally removes retained data.
    tx.execute("DELETE FROM google_tokens WHERE user_id NOT IN (SELECT user_id FROM activations WHERE expires>?1)",[time])?;
    tx.execute("DELETE FROM inbox_cache WHERE user_id NOT IN (SELECT user_id FROM activations WHERE expires>?1)",[time])?;
    tx.execute("DELETE FROM sessions WHERE expires<=?1 OR activation_id IS NULL OR user_id NOT IN (SELECT user_id FROM activations WHERE expires>?1)",[time])?;
    tx.execute("DELETE FROM activations WHERE expires<=?1",[time])?;
    tx.execute("DELETE FROM email_challenges WHERE expires<=?1",[time])?;
    tx.execute("DELETE FROM oauth_states WHERE expires<=?1",[time])?;
    tx.commit()?;Ok(())
}

pub fn store_inbox(s:&AppState,user:&str,generation:&str,payload:&Value) -> Result<()> {
    let sealed=s.seal(user,&payload.to_string())?;
    let mut db=s.db()?;let tx=db.transaction()?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM activations WHERE user_id=?1 AND generation=?2 AND expires>?3)",params![user,generation,now()],|r|r.get(0))?;
    if !exists {return Err(unauthorized());}
    tx.execute("INSERT INTO inbox_cache VALUES(?1,?2,?3,?4) ON CONFLICT(user_id) DO UPDATE SET generation=excluded.generation,payload=excluded.payload,synced_at=excluded.synced_at",params![user,generation,sealed,now()])?;
    tx.execute("UPDATE activations SET last_synced=?1,sync_error=NULL WHERE user_id=?2",params![now(),user])?;
    tx.commit()?;Ok(())
}

pub fn start_workers(s:AppState) {
    let cleaner=s.clone();
    tokio::spawn(async move {
        let mut tick=tokio::time::interval(Duration::from_secs(5));
        loop {tick.tick().await;if cleanup(&cleaner).is_err(){tracing::error!("Activation cleanup failed; expired access remains blocked by request checks");}}
    });
    tokio::spawn(async move {
        let mut tick=tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            if let Err(_)=sync_due(&s).await {tracing::warn!("Inbox synchronization cycle failed");}
        }
    });
}

pub(super) async fn sync_due(s:&AppState) -> Result<()> {
    let due:Vec<(String,String,i64)>={
        let db=s.db()?;let mut query=db.prepare("SELECT a.user_id,a.generation,a.expires FROM activations a JOIN google_tokens g ON g.user_id=a.user_id WHERE a.expires>?1 AND a.next_sync<=?1 ORDER BY a.next_sync LIMIT 8")?;
        let rows=query.query_map([now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;rows
    };
    for (user,generation,expires) in due {
        if now()>=expires {continue;}
        let changed=s.db()?.execute("UPDATE activations SET next_sync=?1 WHERE user_id=?2 AND generation=?3 AND expires>?4 AND next_sync<=?4",params![now()+60,user,generation,now()])?;
        if changed==0 {continue;}
        let result=tokio::time::timeout(Duration::from_secs((expires-now()).max(0) as u64),async {
            let _guard=s.mail_lock.lock().await;
            still_active(s,&user,&generation)?;
            let page=fetch_inbox(s,&user,None).await?;
            store_inbox(s,&user,&generation,&page)
        }).await;
        if !matches!(result,Ok(Ok(()))) {
            s.db()?.execute("UPDATE activations SET sync_error='Inbox sync failed. Check Gmail permissions and try Refresh.' WHERE user_id=?1 AND generation=?2 AND expires>?3",params![user,generation,now()])?;
        }
    }
    Ok(())
}
