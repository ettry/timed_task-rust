use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;
fn main() -> io::Result<()> {
    let config_path = dirs::home_dir()
        .expect("找不到家目录")
        .join(".config/time_event.datafile");

    // 启动时先加载一次
    let mut config_data = load_config(&config_path);

    // 建立 mpsc 通道和 notify watcher
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(move |res| tx.send(res).unwrap(), Config::default())
        .expect("无法创建 watcher");

    // 监控配置文件
    watcher
        .watch(&config_path, RecursiveMode::NonRecursive)
        .expect("无法监控配置文件");
    let mut time_num: i128 = 0;
    let mut booler = false;
    let mut boolar = true;
    loop {
        while let Ok(res) = rx.try_recv() {
            match res {
                Ok(Event {
                    kind: EventKind::Modify(_),
                    ..
                })
                | Ok(Event {
                    kind: EventKind::Create(_),
                    ..
                }) => {
                    // println!("哦哦哦哦哦哦，配置文件发生了变化，重新加载");
                    config_data = load_config(&config_path);
                    boolar = true;
                }
                Ok(_) => {}
                Err(e) => eprintln!("文件监控出错: {:?}", e),
            }
        }

        let mut new_lines: Vec<String> = Vec::new();
        let sh = match std::env::var("SHELL") {
            Ok(shell) => shell,
            Err(_) => {
                eprintln!("环境变量 SHELL 未设置，使用默认的 /bin/sh");
                "/bin/sh".to_string()
            }
        };
        for line in config_data.iter() {
            let mut tem = 0;
            let mut line_string = line.to_string();
            let event_time: Vec<&str> = line.split_whitespace().collect();
            if boolar {
                if event_time[0] != "*" {
                    let num = match event_time[0].parse::<i128>() {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    tem += num;
                } else if event_time[1] == "*" && event_time[2] == "*" {
                    tem = 1;
                }
                if event_time[1] != "*" {
                    let num = match event_time[0].parse::<i128>() {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    tem += num * 60;
                }
                if event_time[2] != "*" {
                    let num = match event_time[0].parse::<i128>() {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    tem += num * 1440; //24*60
                }
                if event_time.len() <= 4 {
                    booler = true;
                    line_string.push_str(&format!(" {}", tem));
                } else if event_time.len() == 5 && tem.to_string() != event_time[4] {
                    booler = true;
                    let mut new_eve: Vec<String> =
                        event_time.iter().map(|s| s.to_string()).collect();
                    new_eve[4] = tem.to_string();
                    line_string = new_eve.join(" ");
                } else {
                    booler = false;
                }
                // line_string.push_str(&format!(" "));
            }
            if boolar {
                new_lines.push(line_string);
            }
            // if boolar {
            //     if time_num == 0 || time_num % tem == 0 {
            //         command(&event_time[3].to_string(), sh.clone());
            //     } else if time_num % event_time[4].parse::<i128>().unwrap() == 0 {
            //         command(&event_time[3].to_string(), sh.clone());
            //     }
            // }
            if time_num == 0
                || (tem != 0 && time_num % tem == 0)
                || (event_time.len() == 5 && time_num % event_time[4].parse::<i128>().unwrap() == 0)
            {
                command(event_time[3], sh.clone());
            }
        }
        boolar = false;

        if booler {
            booler = false;
            // 写回原文件
            let mut file = File::create(&config_path)?;
            for line in new_lines {
                writeln!(file, "{line}")?;
            }
        }

        thread::sleep(Duration::from_secs(60));
        time_num += 1;
    }
}

fn load_config(config_path: &PathBuf) -> Vec<String> {
    // 读取配置文件的每一行，返回 Vec<String>
    let file = match File::open(config_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("打开配置文件失败: {}", e);
            return vec![];
        }
    };
    // let reader = BufReader::new(file);
    // reader.lines().filter_map(Result::ok).collect()
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .take_while(|r| !r.is_err()) // 只要不是错误就继续
        .filter_map(Result::ok)
        .collect();
    lines
}

fn command(event_time_str: &str, sh: String) {
    let escaped_input_path = event_time_str.replace("'", "'\\''");
    let mut cmd = Command::new(sh);
    cmd.arg("-e");
    cmd.arg("-c");
    cmd.arg(&escaped_input_path);
    cmd.stdin(Stdio::null());
    if let Err(e) = cmd.spawn() {
        eprintln!("执行命令失败: {}", e);
    }
}
