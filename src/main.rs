use anyhow::Result;
use async_ssh2_tokio::client::{AuthMethod, Client, ServerCheckMethod};
use pinger::{ping_with_interval, PingResult};
use russh::client::Config;
use std::collections::HashMap;
use std::net::SocketAddrV6;

use network_interface::NetworkInterface;
use network_interface::NetworkInterfaceConfig;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref HWID: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("7AA", "Axis W800");
        m.insert("7DC.1", "Axis W100");
        m.insert("908", "Axis W101 Alpha");
        m.insert("908.1", "Axis W101 Alpha");
        m.insert("908.2", "Axis W101 PS");
        m.insert("908.21", "Axis W101 Black");
        m.insert("908.22", "Axis W101 White");
        m.insert("95F", "Axis W110 Alpha");
        m.insert("95F.1", "Axis W110 Black");
        m.insert("95F.2", "Axis W110 Gray");
        m
    };
}

async fn check_bwc(ip: String, if_index: u32) -> Result<()> {
    let cfg = Config {
        connection_timeout: Some(std::time::Duration::from_secs(3)),
        ..Default::default()
    };

    let ip = ip.split('%').collect::<Vec<&str>>()[0].to_string();

    let client = Client::connect_with_config(
        SocketAddrV6::new(ip.parse()?, 22, 0, if_index),
        "root",
        AuthMethod::with_password("pass"),
        ServerCheckMethod::NoCheck,
        cfg,
    )
    .await?;

    let result = client.execute("bootblocktool -x HWID").await?;

    let mut hwid = result.output;

    hwid.pop();

    if let Some(&model) = HWID.get(&hwid.as_str()) {
        println!("{:15} - {}", model, ip);
    } else {
        println!("Unknow {}", hwid);
    }

    Ok(())
}
struct NetInterface {
    name: String,
    index: u32,
}

impl NetInterface {
    pub fn new(name: String, index: u32) -> Self {
        Self { name, index }
    }

    pub fn search(self) {
        let mut devices = HashMap::new();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let stream = ping_with_interval(
            "ff02::1".to_string(),
            std::time::Duration::from_secs(3),
            Some(self.name.clone()),
        )
        .expect("Error pinging");

        println!("{}", self.name);

        rt.block_on(async {
            for message in stream {
                match message {
                    PingResult::Pong(_, s) => {
                        let mut ip = s.split(' ').collect::<Vec<&str>>()[3].to_string();

                        ip.pop();

                        devices.entry(ip.clone()).or_insert_with(|| {
                            tokio::spawn(async move { check_bwc(ip, self.index).await });
                            false
                        });
                    }
                    PingResult::Timeout(_) => {
                        println!("Ping timeout")
                    }
                    PingResult::Unknown(_) => todo!(),
                    PingResult::PingExited(_, _) => {
                        println!("Ping exit")
                    }
                }
            }
        })
    }
}

fn main() -> Result<()> {
    let network_interfaces = NetworkInterface::show().unwrap();
    let mut handle_array = Vec::new();

    for itf in network_interfaces.iter() {
        /* filter out lo and docker */
        if itf.name.starts_with("lo") || itf.name.starts_with("docker") {
            continue;
        }
        let net = NetInterface::new(itf.name.clone(), itf.index);
        handle_array.push(std::thread::spawn(|| net.search()));
    }

    for thread in handle_array.into_iter() {
        thread.join().unwrap();
    }

    Ok(())
}
