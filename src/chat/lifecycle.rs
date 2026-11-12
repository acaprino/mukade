//! One OS-held owner per canonical data directory; PIDs are diagnostic only.
use super::{ChatService, ServiceConfig, client::ClientConfig, security};
use crate::{bus::{Bus, ChatError}, roots::StoreRoots};
use serde::{Deserialize, Serialize};
use serde_json::{Value,json};
use std::{fs::{self,File,TryLockError},io::{BufRead,BufReader,Read,Write},path::{Path,PathBuf},process::{Command,Stdio},time::{Duration,Instant}};
pub const DEADLINE:Duration=Duration::from_secs(10);
fn failure(message:&str)->ChatError{ChatError::new("transport_failure",message)}
fn storage(_:std::io::Error)->ChatError{failure("Cannot safely access chat storage; select an owned local MUKADE_DATA_DIR outside projects and native stores")}

#[derive(Clone,Debug)]
pub struct Paths {pub data_dir:PathBuf,pub bus_dir:PathBuf}
impl Paths {
    pub fn from_env()->Result<Self,ChatError>{Self::resolve(&crate::portable::data_dir().ok_or_else(||failure("MUKADE_DATA_DIR must be absolute"))?,&StoreRoots::from_env())}
    pub fn resolve(data:&Path,roots:&StoreRoots)->Result<Self,ChatError>{
        let data_dir=security::resolve(data).map_err(storage)?;let bus_dir=security::resolve(&data_dir.join("bus")).map_err(storage)?;
        // The private boundary itself must not be a symlink/junction, even if its target is safe.
        for p in [data,&data.join("bus")] {if let Ok(m)=fs::symlink_metadata(p){if m.file_type().is_symlink(){return Err(storage(std::io::ErrorKind::PermissionDenied.into()));}
            #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if m.file_attributes()&0x400!=0{return Err(storage(std::io::ErrorKind::PermissionDenied.into()));}}
        }}
        let vars=crate::catalog::Vars::from_env();
        for p in [&data_dir,&bus_dir] {
            for a in p.ancestors(){match fs::symlink_metadata(a.join(".git")){Ok(_)=>return Err(failure("Chat data directory is inside a project; set an external MUKADE_DATA_DIR")),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(storage(e))}}
            for h in crate::HarnessId::ALL {if let Some(mut root)=roots.root(h).map(Path::to_owned).or_else(||crate::catalog::store_default_with(h,&vars)) {if h==crate::HarnessId::OpenCode&&root.extension().is_some(){root.pop();}if !root.is_absolute(){root=std::path::absolute(root).map_err(storage)?;}if security::contains(&security::resolve(&root).map_err(storage)?,p){return Err(failure("Chat data directory is inside a native harness store"));}}}
            for var in ["MUKADE_WORKSPACE_ROOTS","MUKADE_SYNC_ROOTS","OneDrive","OneDriveCommercial","OneDriveConsumer","DROPBOX_HOME","GOOGLE_DRIVE_HOME"] {if let Some(value)=std::env::var_os(var){for root in std::env::split_paths(&value){if root.as_os_str().is_empty(){continue;}let root=security::resolve(&root).map_err(storage)?;if security::contains(&root,p){return Err(failure("Chat data directory is inside a configured workspace or synchronized folder"));}}}}
            security::ensure_local(p).map_err(storage)?;
        }
        Ok(Self{data_dir,bus_dir})
    }
    pub fn state_path(&self)->PathBuf{self.bus_dir.join("instance.json")}
    pub fn credential_path(&self)->PathBuf{self.bus_dir.join("credential")}
}

pub struct Owner {paths:Paths,_lock:File,_directory:File}
impl Owner {
    pub fn acquire(paths:&Paths)->Result<Self,ChatError>{
        security::create_private_directory(&paths.data_dir).map_err(storage)?;
        security::create_private_directory(&paths.bus_dir).map_err(storage)?;
        let directory=security::hold_directory(&paths.bus_dir).map_err(storage)?;
        if !security::contains(&paths.bus_dir,&paths.bus_dir.canonicalize().map_err(storage)?){return Err(failure("Chat directory changed during startup"));}
        let lock_path=paths.bus_dir.join("instance.lock");
        if lock_path.exists(){security::protect_file(&lock_path).map_err(storage)?;}
        let lock=security::open_lock(&lock_path,true).map_err(storage)?;
        match lock.try_lock(){Ok(())=>{},Err(TryLockError::WouldBlock)=>return Err(ChatError::new("already_running","Another process owns this chat data directory")),Err(TryLockError::Error(e))=>return Err(storage(e))}
        // Migrate known legacy application files only, after owning the instance lock.
        for e in fs::read_dir(&paths.bus_dir).map_err(storage)? {let e=e.map_err(storage)?;let name=e.file_name();let name=name.to_string_lossy();
            if matches!(name.as_ref(),"bus.db"|"bus.db-wal"|"bus.db-shm"|"bus.db-journal"|"schema.lock"|"config.json"|"credential"|"instance.json")||name.starts_with(".publish-"){security::protect_file(&e.path()).map_err(storage)?;}
            else if name=="backups" {protect_backups(&e.path())?;}else{security::verify_file(&e.path()).map_err(storage)?;}
        }
        Ok(Self{paths:paths.clone(),_lock:lock,_directory:directory})
    }
    pub fn clear_instance(&self,id:&str)->Result<(),ChatError>{if let Some(state)=read_state(&self.paths)?{if state.instance_id==id{fs::remove_file(self.paths.state_path()).map_err(storage)?;}}Ok(())}
}
fn protect_backups(p:&Path)->Result<(),ChatError>{security::create_private_directory(p).map_err(storage)?;for e in fs::read_dir(p).map_err(storage)?{let e=e.map_err(storage)?;if e.file_type().map_err(storage)?.is_dir(){protect_backups(&e.path())?;}else{security::protect_file(&e.path()).map_err(storage)?;}}Ok(())}

/// Read-only probe: never create a path or truncate the persistent lock.
pub fn owner_active(paths:&Paths)->Result<bool,ChatError>{
    match fs::symlink_metadata(&paths.bus_dir){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(false),Err(e)=>return Err(storage(e)),Ok(_)=>security::verify_directory(&paths.bus_dir).map_err(storage)?}
    let lock=match security::open_lock(&paths.bus_dir.join("instance.lock"),false){Ok(f)=>f,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(false),Err(e)=>return Err(storage(e))};
    match lock.try_lock(){Ok(())=>Ok(false),Err(TryLockError::WouldBlock)=>Ok(true),Err(TryLockError::Error(e))=>Err(storage(e))}
}
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {pub data_dir:PathBuf,pub endpoint:String,pub pid:u32,pub service_id:String,pub instance_id:String,pub server_version:String}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct Config {port:u16}
pub fn read_state(paths:&Paths)->Result<Option<Instance>,ChatError>{
    let bytes=match security::read_private(&paths.state_path()){Ok(v)=>v,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(storage(e))};
    let state:Instance=serde_json::from_slice(&bytes).map_err(|_|failure("Invalid chat instance metadata"))?;
    if state.data_dir!=paths.data_dir||uuid::Uuid::parse_str(&state.instance_id).is_err(){return Err(failure("Chat instance metadata does not match this directory"));}Ok(Some(state))
}
/// Protected credential/config primitive for explicit harness connection commands. No token is logged.
pub fn client_config(paths:&Paths)->Result<ClientConfig,ChatError>{
    if !owner_active(paths)?{return Err(failure("Chat service is stopped; run `mukade chat start`"));}
    let state=read_state(paths)?.ok_or_else(||failure("Chat service is starting or unreachable; inspect `mukade chat status`"))?;
    let token=read_credential(paths)?;
    // Validate the URL before a caller is allowed to send the credential.
    ServiceConfig::new(paths.data_dir.clone(),state.service_id.clone(),state.instance_id.clone(),state.endpoint.clone(),token.clone())?;
    Ok(ClientConfig{endpoint:state.endpoint,token,service_id:state.service_id,instance_id:state.instance_id})
}
pub fn read_credential(paths:&Paths)->Result<String,ChatError>{
    security::verify_directory(&paths.bus_dir).map_err(storage)?;
    let token=String::from_utf8(security::read_private(&paths.credential_path()).map_err(storage)?).map_err(|_|failure("Invalid chat credential file"))?;
    if token.len()!=64||!token.bytes().all(|b|b.is_ascii_hexdigit()){return Err(failure("Invalid chat credential file"));}Ok(token)
}
pub async fn status(paths:&Paths)->Result<Value,ChatError>{
    if !owner_active(paths)?{return Ok(json!({"status":"stopped","data_dir":paths.data_dir}));}
    let state=read_state(paths)?;
    let running=match client_config(paths){Ok(c)=>c.health().await.is_ok_and(|h|h.status=="running"),Err(_)=>false};
    let mut v=state.map(|s|serde_json::to_value(s).unwrap()).unwrap_or_else(||json!({"data_dir":paths.data_dir}));v["status"]=json!(if running{"running"}else{"unreachable"});Ok(v)
}
fn check_port(state:&Instance,port:Option<u16>)->Result<(),ChatError>{if let Some(p)=port{let cfg=ServiceConfig::new(state.data_dir.clone(),state.service_id.clone(),state.instance_id.clone(),state.endpoint.clone(),"validation".into())?;if cfg.listen_addr().port()!=p{return Err(ChatError::new("configuration_conflict","The running service uses another port; stop it before changing --port"));}}Ok(())}

pub async fn start(paths:&Paths,port:Option<u16>)->Result<Value,ChatError>{start_with_executable(paths,port,&std::env::current_exe().map_err(storage)?).await}
pub async fn start_with_executable(paths:&Paths,port:Option<u16>,exe:&Path)->Result<Value,ChatError>{
    let deadline=Instant::now()+DEADLINE;
    loop {
        if owner_active(paths)? {
            if let Some(state)=read_state(paths)?{check_port(&state,port)?;if let Ok(config)=client_config(paths){if let Ok(Ok(health))=tokio::time::timeout(deadline.saturating_duration_since(Instant::now()),config.health()).await {if health.status=="running"{let mut v=serde_json::to_value(state).unwrap();v["status"]=json!("running");return Ok(v);}}}}
            if Instant::now()>=deadline{return Err(failure("Chat instance is locked but unreachable; no second service was started"));}
            tokio::time::sleep(Duration::from_millis(50)).await;continue;
        }
        let spawn=uuid::Uuid::new_v4().to_string();
        let mut command=Command::new(exe);command.args(["chat","serve","--parent-id",&spawn]).env("MUKADE_DATA_DIR",&paths.data_dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Some(port)=port{command.args(["--port",&port.to_string()]);}
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);}
        let mut child=command.spawn().map_err(|_|failure("Could not spawn chat service"))?;
        let stdout=child.stdout.take().unwrap();let (tx,rx)=std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move||{let mut line=String::new();let result=BufReader::new(stdout).take(16385).read_line(&mut line).map(|_|line);let _=tx.send(result);});
        let ready=rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        let accepted=(||->Result<Instance,ChatError>{
            let line=ready.map_err(|_|failure("Chat startup readiness timed out"))?.map_err(storage)?;
            if line.len()>16384{return Err(failure("Invalid chat startup readiness"));}
            let state:Instance=serde_json::from_str(&line).map_err(|_|failure("Chat service failed before readiness"))?;
            if state.instance_id!=spawn||state.pid!=child.id()||state.data_dir!=paths.data_dir{return Err(failure("Chat startup identity mismatch"));}
            let recorded=read_state(paths)?.ok_or_else(||failure("Chat readiness metadata missing"))?;
            if recorded.instance_id!=spawn||recorded.endpoint!=state.endpoint||recorded.service_id!=state.service_id||!owner_active(paths)?{return Err(failure("Chat startup metadata mismatch"));}
            check_port(&state,port)?;let _=client_config(paths)?;
            if Instant::now()>=deadline{return Err(failure("Chat startup deadline expired"));}
            child.stdin.as_mut().ok_or_else(||failure("Chat startup pipe closed"))?.write_all(b"COMMIT\n").map_err(storage)?;
            Ok(state)
        })();
        match accepted {
            Ok(state)=>{drop(child.stdin.take());let mut v=serde_json::to_value(state).unwrap();v["status"]=json!("running");return Ok(v);}
            Err(e)=>{drop(child.stdin.take());let _=child.kill();let _=child.wait();
                // The only PID ever terminated is our still-owned Child handle.
                if !owner_active(paths).unwrap_or(true){if let Ok(owner)=Owner::acquire(paths){let _=owner.clear_instance(&spawn);}}
                if owner_active(paths).unwrap_or(false)&&Instant::now()<deadline{continue;}return Err(e);
            }
        }
    }
}
pub async fn stop(paths:&Paths)->Result<Value,ChatError>{
    let deadline=Instant::now()+DEADLINE;
    if !owner_active(paths)?{return Ok(json!({"status":"stopped","data_dir":paths.data_dir}));}
    let config=client_config(paths)?;let instance=config.instance_id.clone();
    tokio::time::timeout(DEADLINE,config.shutdown()).await.map_err(|_|failure("Chat shutdown request timed out"))??;
    loop {if !owner_active(paths)?||read_state(paths)?.is_some_and(|s|s.instance_id!=instance){return Ok(json!({"status":"stopped","instance_id":instance,"data_dir":paths.data_dir}));}
        if Instant::now()>=deadline{return Err(failure("Chat shutdown deadline expired; the owned instance has not stopped"));}tokio::time::sleep(Duration::from_millis(25)).await;}
}
/// Foreground owner. Caller must terminate its own process after a deadline error;
/// a runtime may still own blocking database work. Never invoke process termination here.
pub async fn serve(paths:Paths,port:Option<u16>,parent_id:Option<String>)->Result<(),ChatError>{
    let owner=Owner::acquire(&paths)?;
    let instance=parent_id.clone().unwrap_or_else(||uuid::Uuid::new_v4().to_string());
    let result=serve_owned(&owner,port,parent_id,instance.clone()).await;
    let cleanup=owner.clear_instance(&instance);result.and(cleanup)
}
async fn serve_owned(owner:&Owner,port:Option<u16>,parent_id:Option<String>,instance:String)->Result<(),ChatError>{
    let paths=&owner.paths;
    let saved=match security::read_private(&paths.bus_dir.join("config.json")){Ok(bytes)=>Some(serde_json::from_slice::<Config>(&bytes).map_err(|_|failure("Invalid chat configuration"))?),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,Err(e)=>return Err(storage(e))};
    if saved.as_ref().is_some_and(|c|c.port==0){return Err(failure("Invalid persisted chat port"));}
    let listener=tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,port.or(saved.map(|c|c.port)).unwrap_or(0))).await.map_err(|_|failure("Chat loopback port is unavailable; stop the conflicting listener or explicitly choose --port"))?;
    let bound_port=listener.local_addr().map_err(storage)?.port();
    let token=if paths.credential_path().exists(){read_credential(paths)?}else{let token=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());security::atomic_write(&paths.credential_path(),token.as_bytes()).map_err(storage)?;token};
    let dir=paths.bus_dir.clone();let bus=tokio::task::spawn_blocking(move||Bus::open_in(&dir)).await.map_err(|_|failure("Chat database initialization failed"))?.map_err(ChatError::from)?;
    let state=Instance{data_dir:paths.data_dir.clone(),endpoint:format!("http://127.0.0.1:{bound_port}/mcp"),pid:std::process::id(),service_id:bus.service_id()?,instance_id:instance,server_version:env!("CARGO_PKG_VERSION").into()};
    let service=ChatService::new(bus,StoreRoots::from_env(),ServiceConfig::new(state.data_dir.clone(),state.service_id.clone(),state.instance_id.clone(),state.endpoint.clone(),token)?)?;
    for name in ["bus.db","bus.db-wal","bus.db-shm","schema.lock"]{let p=paths.bus_dir.join(name);if p.exists(){security::protect_file(&p).map_err(storage)?;}}
    security::atomic_write(&paths.bus_dir.join("config.json"),&serde_json::to_vec(&Config{port:bound_port}).unwrap()).map_err(storage)?;
    security::atomic_write(&paths.state_path(),&serde_json::to_vec(&state).unwrap()).map_err(storage)?;
    if parent_id.is_some(){
        let mut out=std::io::stdout().lock();writeln!(out,"{}",serde_json::to_string(&state).unwrap()).map_err(storage)?;out.flush().map_err(storage)?;drop(out);
        let(tx,rx)=tokio::sync::oneshot::channel();std::thread::spawn(move||{let mut line=String::new();let result=std::io::stdin().lock().take(16).read_line(&mut line).map(|_|line);let _=tx.send(result);});
        let gate=tokio::time::timeout(DEADLINE,rx).await.map_err(|_|failure("Chat startup parent deadline expired"))?.map_err(|_|failure("Chat startup parent disappeared"))?.map_err(storage)?;
        if gate!="COMMIT\n"{return Err(failure("Chat startup was not committed by its parent"));}
    }
    // Never expose a healthy endpoint while the parent can still abort its child.
    let serving=tokio::spawn({let s=service.clone();async move{s.serve(listener).await}});
    tokio::select!{_ = service.stopping_token().cancelled_owned()=>{},_ = tokio::signal::ctrl_c()=>{service.begin_shutdown();}}
    if tokio::time::timeout(DEADLINE,async {service.finish_shutdown().await;let _=serving.await;}).await.is_err(){service.transport_token().cancel();return Err(failure("Chat shutdown deadline expired; terminating the owned service process"));}
    Ok(())
}
