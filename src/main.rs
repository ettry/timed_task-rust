//更精确的时间休眠 ✔️
//固定时间扩展到秒分时天月 ✔️
//间隔时间扩展到秒分时天月 ✔️
//间隔时间可以设置是否立即运行命令 ✔️
//修bug ✔️
//未找到文件时创建文件 ✔️
//收集错误转储为日志 ✔️

use chrono::{DateTime, Local};
use gag::Redirect;
use notify::{Config as nConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::{thread, time::Duration as StdDuration};
enum IntervalOrFixedTime {
    Fixed(String),
    Interval(i128),
}

struct Options {
    interval_or_fixed: IntervalOrFixedTime,
    start_run: bool,
    run_command: String,
}

fn main() -> io::Result<()> {
    let log_path = dirs::home_dir()
        .expect("找不到家目录")
        .join(".config/time-event/te.log");
    let config_path = dirs::home_dir()
        .expect("找不到家目录")
        .join(".config/time-event/te.conf");

    if let Some(parent) = config_path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        eprintln!("创建目录失败: {}", err);
        return Err(err);
    }
    // 打开或创建日志文件
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    // 重定向 stderr 到日志文件
    let _redirect = Redirect::stderr(log_file)?;
    // 启动时先加载一次
    let mut config_data = match load_config(&config_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("无法读取配置文件: {}", e);
            return Err(e);
        }
    };
    let mut configs = Vec::new();
    // 建立 mpsc 通道和 notify watcher
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(move |res| tx.send(res).unwrap(), nConfig::default())
        .expect("无法创建 watcher");

    // 监控配置文件
    watcher
        .watch(&config_path, RecursiveMode::NonRecursive)
        .expect("无法监控配置文件");
    //启动时间
    let mut time_num: i128 = 0;
    //文件是否被修改
    let mut boolar = true;
    loop {
        let start_time = Local::now();
        while let Ok(res) = rx.try_recv() {
            if let Some(new_config) = handle_event(res, &config_path) {
                config_data = new_config;
                boolar = true;
            }
        }

        let sh = match std::env::var("SHELL") {
            Ok(shell) => shell,
            Err(_) => {
                eprintln!("环境变量 SHELL 未设置，使用默认的 /bin/sh");
                "/bin/sh".to_string()
            }
        };

        if boolar {
            configs.clear();
            for line in config_data.iter() {
                //当前行时间
                let mut tem = 0;
                //let event_time: Vec<&str> = line.split_whitespace().collect();
                //当文件被修改时
                //*表示时间段
                //间隔时间格式: "*" [m] [h] [d] [cmd]   len = 5
                //固定时间格式: ":" [run_time] [cmd]    len = 3
                if line.starts_with("*") {
                    let event_time: Vec<String> = line
                        .splitn(8, char::is_whitespace)
                        .map(|s| s.to_string())
                        .collect();
                    //判断时间或是否为固定时间
                    if event_time.get(2).map_or("*", |v| v) != "*"
                        || event_time.get(3).map_or("*", |v| v) != "*"
                        || event_time.get(4).map_or("*", |v| v) != "*"
                        || event_time.get(5).map_or("*", |v| v) != "*"
                        || event_time.get(6).map_or("*", |v| v) != "*"
                    {
                        let mut jjk = 0;
                        for s in event_time.iter().take(7).skip(2) {
                            match s.parse::<i128>() {
                                Ok(v) => {
                                    tem += match jjk {
                                        0 => v,
                                        1 => v * 60,      //分钟
                                        2 => v * 3600,    //小时
                                        3 => v * 86400,   //天
                                        4 => v * 2592000, //月，按30天计算
                                        _ => continue,
                                    }
                                }
                                Err(_) => print!("*"),
                            }
                            jjk += 1;
                        }
                    } else {
                        tem = 1;
                    }
                    let tstr = event_time.get(1).map(|s| s.as_str()).unwrap_or("*");
                    //tstr为是否在软件运行后执行命令的字符串切片版本
                    configs.push(Options {
                        interval_or_fixed: IntervalOrFixedTime::Interval(tem),
                        start_run: tstr.to_lowercase().starts_with("y"),
                        // matches!(event_time.get(1).map(|s| s.as_str()), Some("y")),
                        run_command: event_time
                            .get(7)
                            .map(|s| s.to_string()) // Option<String>
                            .unwrap_or_else(|| "echo 'config error'".to_string()),
                    });
                } else if line.starts_with(":") {
                    let event_time: Vec<String> = line
                        .splitn(3, char::is_whitespace)
                        .map(|s| s.to_string())
                        .collect();
                    //固定时间运行不做处理
                    configs.push(Options {
                        interval_or_fixed: IntervalOrFixedTime::Fixed(
                            event_time
                                .get(1) // Option<&&str>
                                .map(|s| s.to_string()) // Option<String>
                                .unwrap_or_else(|| {
                                    eprintln!("配置错误：缺少 run_time，使用当前时间作为默认值");
                                    Local::now().format("%d:%H:%M:%S").to_string()
                                }),
                        ),
                        start_run: false,
                        run_command: event_time.get(2).map(|s| s.to_string()).unwrap_or_else(
                            || {
                                eprintln!("配置错误：缺少 run_command，使用占位命令");
                                "echo 'config error'".to_string()
                            },
                        ),
                    });
                } else {
                    continue;
                }
            }
        }
        boolar = false;
        for run_conf in &configs {
            //判断间隔时间运行命令是否可以运行
            if let IntervalOrFixedTime::Interval(interval) = run_conf.interval_or_fixed
                && ((run_conf.start_run && time_num == 0)
                    || (time_num != 0 && time_num % interval == 0))
            {
                command(&run_conf.run_command, sh.clone());
            }
            if let IntervalOrFixedTime::Fixed(fixed_running_time) = &run_conf.interval_or_fixed
                && local_time_in(fixed_running_time)
            {
                //判断固定时间运行命令是否可以运行
                command(&run_conf.run_command, sh.clone());
            }
        }

        time_num += 1;
        sleep_time(start_time, 1);
    } //loop
}

fn load_config(config_path: &PathBuf) -> io::Result<Vec<String>> {
    // 尝试打开文件
    let file = match File::open(config_path) {
        Ok(f) => f,
        Err(e) => {
            // 如果错误是“文件未找到”，则尝试创建
            if e.kind() == io::ErrorKind::NotFound {
                eprintln!("配置文件不存在，尝试创建: {:?}", config_path);
                let mut file = File::create(config_path)?;
                // 写入默认内容
                writeln!(
                    file,
                    "# 间隔时间格式为 *  是否启动软件时执行(y/n)  秒  分  时  天  月(30天)  命令"
                )?;
                writeln!(file, "# 例: * y 3 * 1 0 2 echo \"hello world\"")?;
                writeln!(
                    file,
                    "# 允许软件启动时执行，自软件启动起每隔3秒0分钟1小时0天2月(60天)执行一次命令，*和0一个意思代表空，执行echo \"hello world\""
                )?;
                writeln!(file, "# 固定时间格式为 :  天:时:分:秒(无空格)   命令")?;
                writeln!(file, "# 固定时间其他格式为 时:分:秒 / 分:秒 / 秒")?;
                writeln!(file, "# 例: 30:15 ~/sh/hellorust.sh")?;
                writeln!(file, "# 每个30分15秒运行 ~/sh/hellorust.sh 文件")?;

                eprintln!("已创建新的配置文件并写入默认内容");
                eprintln!("请编辑该文件以添加您的时间事件配置");
                eprintln!("文件路径: {:?}", config_path);
                // 因为刚写入，直接返回默认内容即可，不需要重新读取文件
                return Ok(vec!["hello rust".to_string()]);
            }

            // 如果是其他错误（如权限不足），直接返回错误
            return Err(e);
        }
    };

    // 读取文件内容
    let reader = BufReader::new(file);

    // collect() 会自动处理 Result，如果遇到读取错误会返回 Err
    reader.lines().collect()
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

fn local_time_in(event_time: &str) -> bool {
    let mut local_time_bool = false;
    let count = event_time.matches(':').count();
    let local_time = match count {
        0 => Local::now().format("%S"),
        1 => Local::now().format("%M:%S"),
        2 => Local::now().format("%H:%M:%S"),
        3 => Local::now().format("%d:%H:%M:%S"),
        4 => Local::now().format("%m:%d:%H:%M:%S"),
        _ => return local_time_bool,
    };
    if local_time.to_string() == event_time {
        local_time_bool = true;
    }
    local_time_bool
}

fn sleep_time(start_time: DateTime<Local>, sleep_time_in: i32) {
    let start_run = start_time + Duration::from_secs(sleep_time_in as u64);
    let tem_intime = Local::now();
    let minis = (start_run - tem_intime).num_milliseconds();
    if minis > 0 {
        thread::sleep(StdDuration::from_millis(minis as u64));
    }
}

fn handle_event(event: Result<Event, notify::Error>, config_path: &PathBuf) -> Option<Vec<String>> {
    if let Ok(ev) = event {
        let hit = ev.paths.iter().any(|p| p == config_path);
        if hit {
            match ev.kind {
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                    let new_config = match load_config(&config_path.to_path_buf()) {
                        Ok(data) => data,
                        Err(e) => {
                            eprintln!("无法读取配置文件: {}", e);
                            return None;
                        }
                    };
                    if !new_config.is_empty() {
                        return Some(new_config);
                    }
                }
                _ => {}
            }
        }
    }
    None
}
