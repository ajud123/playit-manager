use json::JsonValue;
use reqwest::header;
use futures::executor::block_on;
use std::{fs::{self}, io::{self, Write}, path::PathBuf};
use console::{Term, style};

enum Screen {
    Overview, Detailed
}

struct ManagerState {
    current_screen: Screen,
    current_option: usize,
    choice_history: Vec<usize>,
    max_options: usize
}

struct PlayitManager {
    client: reqwest::Client,
    cached_data: JsonValue,
    is_dirty: bool,
    is_logged_in: bool
}

impl PlayitManager {
    fn new() -> PlayitManager {
        return PlayitManager {
            client: reqwest::Client::new(),
            cached_data: JsonValue::new_object(),
            is_dirty: true,
            is_logged_in: false
        };
    }
    async fn login(&mut self, sessionid: &str) -> bool {
        let mut builder = reqwest::Client::builder();
        let mut headers = header::HeaderMap::new();
        let cookie = format!("__session={}", sessionid);
        let test2 = cookie.as_str();
        headers.insert("Cookie", header::HeaderValue::from_str(test2).unwrap());
        builder = builder.default_headers(headers);
        let built = builder.build();    
        self.client = built.unwrap();
        let data = self.get_account_info().await;
        if data.is_err() {
            return false;
        }
        return true;
    }
    async fn login_with_credentials(&mut self, username: &str, password: &str) -> bool {
        let temp_client = reqwest::Client::new();
        let res = temp_client.post("https://playit.gg/login?_data=routes%2Flogin")
            .body(format!("_action=login&email={}&password={}", username, password))
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .send()
            .await;
        if res.is_err() {
            return false;
        }
        let data = res.unwrap();
        let cookie_data = data.headers().get("set-cookie");

        let raw = cookie_data.unwrap().to_str().unwrap();
        let mut raw_data = raw.split(';');
        let mut session = raw_data.next().unwrap().split('=');
        session.next();
        let cookie = session.next().unwrap();

        let status = self.login(cookie).await;
        if !status{
            return false;
        }
        self.is_logged_in = true;
        return true;
    }
    async fn rename_tunnel(&mut self, idx: usize, new_name: String) -> bool {
        let req_body = format!("name={}", new_name);
        let res = self.client.post(format!("https://playit.gg/account/tunnels/{}/rename?_data=routes%2Faccount%2Ftunnels%2F%24tunnelId%2Frename", self.cached_data["tunnels"]["tunnels"][idx]["id"].to_string()))
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .body(req_body.clone())
            .send().await;
        if res.is_err() {
            return false;
        }
        let response = res.unwrap();
        if response.status().as_u16() == 200 {
            self.is_dirty = true;
            let _ = self.get_account_info().await;
            return true;
        }
        return false;
    }
    async fn change_port(&mut self, idx: usize, new_port: String) -> bool {
        let tunnel = &self.cached_data["tunnels"]["tunnels"][idx];
        let local_port = tunnel["origin"]["data"]["local_port"].to_string();
        let local_ip = tunnel["origin"]["data"]["local_ip"].to_string();

        let res = self.client.post(format!("https://playit.gg/account/tunnels/{}?_data=routes%2Faccount%2Ftunnels%2F%24tunnelId", tunnel["id"].to_string()))
            .header("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8")
            .body(format!("_action=local-address&agent_id={}&local_ip_og={}&local_port_og={}&local_ip=&local_port={}", tunnel["id"].to_string(), local_ip, local_port, new_port))
            .send().await;
        if res.is_err() {
            return false;
        }
        let response = res.unwrap();
        if response.status().as_u16() == 200 {
            self.is_dirty = true;
            let _ = self.get_account_info().await;
            return true;
        }
        return false;
    }
    async fn get_account_info(&mut self) -> Result<JsonValue, &'static str> {
        let response = self.client.get("https://playit.gg/account/?_data=routes%2Faccount").send().await.expect("msg");
        let status = response.status();
        match status.as_u16() {
            200 => {
                // println!("Success!");
            }
            204 => {
                return Err("Not logged in.")
            }
            _ => {
                return Err("Unknown error.")
            },
        }
        let body: String = response.text().await.expect("msg");
        let parsed = json::parse(body.as_str()).unwrap();
            self.cached_data = parsed.clone();
        self.is_dirty = false;
        Ok(parsed)
    }
    async fn get_tunnels(&mut self, selected: usize) -> Result<(), &'static str> {
        let mut parsed = self.cached_data.clone();
        if self.is_dirty {
            parsed = self.get_account_info().await?;
        }
        let mut idx = 0;
        // println!("Tunnels: \n{}\n", dump);
        while !parsed["tunnels"]["tunnels"][idx].is_null() {
            let tunnel = &parsed["tunnels"]["tunnels"][idx];
            let tunnelname = tunnel["name"].dump();
            // let domain = tunnel["alloc"]["data"]["assigned_domain"].dump();
            // let localport = tunnel["origin"]["data"]["local_port"].dump();
            if idx == selected {
                println!("Tunnel {}", style(tunnelname).bold());
            } else {
                println!("Tunnel {}", tunnelname);
            }
            idx+= 1;
        }
        Ok(())
    }
    fn display_tunnel(&self, state: &ManagerState) {
        let tunnel = &self.cached_data["tunnels"]["tunnels"][state.choice_history[0]];
        let tunnelid = tunnel["id"].dump();
        let tunnelname = tunnel["name"].to_string();
        let domain = tunnel["alloc"]["data"]["assigned_domain"].to_string();
        let localport = tunnel["origin"]["data"]["local_port"].to_string();
        println!("Tunnel {}", tunnelid);
        if state.current_option == 0 { println!("- Domain: {}", style(domain).bold()); } else { println!("- Domain: {}", domain); }
        if state.current_option == 1 { println!("- Name: {}", style(tunnelname).bold()); } else { println!("- Name: {}", tunnelname); }
        if state.current_option == 2 { println!("- Local port: {}", style(localport).bold()); } else { println!("- Local port: {}", localport); }
    }
    fn display_state(&mut self, state: &mut ManagerState){
        match state.current_screen {
            Screen::Overview => {
                let future = self.get_tunnels(state.current_option);
                let _ = block_on(future);
                state.max_options = self.cached_data["tunnels"]["tunnels"].len() - 1
            }
            Screen::Detailed => {
                self.display_tunnel(state);
                state.max_options = 2
            }
        }
    }
}

#[tokio::main]      
async fn main() {
    let mut mngr = PlayitManager::new();
    let mut fullpath: PathBuf = std::env::current_dir().unwrap();
    let datapathoption = home::home_dir();
    match datapathoption {
        Some(datapath) => {
            fullpath = datapath;
        }
        None => {}
    }
    fullpath.push(".config");
    fullpath.push("playit-manager");

    let _ = std::fs::create_dir_all(fullpath.clone());
    fullpath.push("auth.conf");
    let data_result = fs::read_to_string(fullpath.clone());
    if data_result.is_ok() {
        let data = data_result.unwrap();
        let parsed = json::parse(data.as_str());
        if parsed.is_ok() {
            let login_data = parsed.unwrap();
            let email = login_data["email"].as_str().unwrap();
            let password = login_data["password"].as_str().unwrap();
            mngr.login_with_credentials(email, password).await;
        }
    }

    let term = Term::stdout();
    let mut key: console::Key;
    let mut stdout = io::stdout();
    let mut state = ManagerState {
        current_screen: Screen::Overview,
        current_option: 0,
        max_options: 0,
        choice_history: Vec::new()
    };
    loop {
        let _ = Term::clear_screen(&term);
        if !mngr.is_logged_in {
            println!("Please log in. (NOTE: CREDENTIALS WILL BE STORED IN PLAINTEXT)");
            print!("Email: ");
            let _ = stdout.flush();
            let email = term.read_line().unwrap();
            print!("Password: ");
            let _ = stdout.flush();
            let passwd = term.read_secure_line().unwrap();

            if mngr.login_with_credentials(email.as_str(), passwd.as_str()).await {
                let mut object = JsonValue::new_object();
                object["email"] = email.into();
                object["password"] = passwd.into();
                let _ = fs::write(fullpath.clone(), object.dump());
            }
        }
        let _ = mngr.display_state(&mut state);
        key = Term::read_key(&term).expect("msg");
        if key == console::Key::Enter || key == console::Key::ArrowRight {
            match state.current_screen {
                Screen::Overview => {
                    state.current_screen = Screen::Detailed;
                    state.choice_history.push(state.current_option);
                    state.current_option = 0;
                }
                Screen::Detailed => {
                    match state.current_option {
                        1 => {
                            term.move_cursor_to(0, 2).unwrap();
                            print!("- Name ({}): ", mngr.cached_data["tunnels"]["tunnels"][state.choice_history[0]]["name"].to_string());
                            let new_name = term.read_line().unwrap();
                            if new_name != "" {
                                let status = mngr.rename_tunnel(state.choice_history[0], new_name).await;
                                if status == false {
                                    println!("Failed to change name!");
                                    loop {}
                                }
                                continue;
                            }
                        }
                        2 => {
                            term.move_cursor_to(0, 3).unwrap();
                            print!("- Local port ({}): ", mngr.cached_data["tunnels"]["tunnels"][state.choice_history[0]]["origin"]["data"]["local_port"].to_string());
                            let new_port = term.read_line().unwrap();
                            if new_port != "" {
                                let status = mngr.change_port(state.choice_history[0], new_port).await;
                                if status == false {
                                    println!("Failed to change port!");
                                    loop {}
                                }
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if key == console::Key::Escape || key == console::Key::ArrowLeft {
            state.current_screen = Screen::Overview;
            let option = state.choice_history.pop();
            if !option.is_none(){
                state.current_option = option.unwrap();
            }
        }
        if key == console::Key::ArrowDown && state.current_option < state.max_options {
            state.current_option+= 1;
        }
        if key == console::Key::ArrowUp && state.current_option > 0 {
            state.current_option-= 1;
        }
    }
}
