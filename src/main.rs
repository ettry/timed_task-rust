//更精确的时间休眠 ✔️
//固定时间扩展到秒分时天月 ✔️
//间隔时间扩展到秒分时天月 ✔️
//间隔时间可以设置是否立即运行命令 ✔️
//修bug ✔️
//未找到文件时创建文件 ✔️
//收集错误转储为日志 ✔️
//添加固定时间运行在软件启动时是否根据条件运行✔️
//文本更新✔️

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
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("IO 错误: {0}")]
    IO(#[from] std::io::Error),

    #[error("配置文件监听错误: {0}")]
    Notify(#[from] notify::Error),

    #[error("重定向 stderr 失败: {0}")]
    Redirect(#[from] gag::RedirectError<std::fs::File>),
}

pub type MEResult<T> = std::result::Result<T, MyError>;

enum IntervalOrFixedTime {
    Fixed(String),
    Interval(i128),
}

struct Options {
    interval_or_fixed: IntervalOrFixedTime,
    start_run: bool,
    run_command: String,
    old_time: String,
}

fn main() -> MEResult<()> {
    let mut appstart = true;
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
        return Err(MyError::IO(err));
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
            return Err(MyError::IO(e));
        }
    };
    let mut configs = Vec::new();
    // 建立 mpsc 通道和 notify watcher
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        nConfig::default(),
    )?;
    // let mut watcher = RecommendedWatcher::new(move |res| tx.send(res).unwrap(), nConfig::default())
    //     .expect("无法创建 watcher");
    // 监控配置文件
    // watcher
    //     .watch(&config_path, RecursiveMode::NonRecursive)
    //     .expect("无法监控配置文件");
    let watch_target = config_path
        .parent()
        .ok_or_else(|| io::Error::other("配置文件没有父目录"))?;

    watcher.watch(watch_target, RecursiveMode::NonRecursive)?;
    //启动时间
    let mut time_num: i128 = 0;
    //文件是否被修改
    let mut document_modification = true;
    loop {
        let start_time = Local::now();
        while let Ok(res) = rx.try_recv() {
            if let Some(new_config) = handle_event(res, &config_path) {
                config_data = new_config;
                document_modification = true;
            }
        }

        let sh = match std::env::var("SHELL") {
            Ok(shell) => shell,
            Err(_) => {
                eprintln!("环境变量 SHELL 未设置，使用默认的 /bin/sh");
                "/bin/sh".to_string()
            }
        };

        if document_modification {
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
                    if tem == 0 {
                        tem = 1;
                        eprintln!("{}行的时间不可以设置为0，已修改为每秒触发1次", line);
                    }
                    let tstr = event_time.get(1).map(|s| s.as_str()).unwrap_or("*");
                    //tstr为是否在软件运行后执行命令的字符串切片版本
                    configs.push(Options {
                        interval_or_fixed: IntervalOrFixedTime::Interval(tem),
                        //判断第二段字符串的第一个字母是不是y如果是y那就需要在软件启动时运行命令
                        start_run: tstr.to_lowercase().starts_with("y"),
                        // matches!(event_time.get(1).map(|s| s.as_str()), Some("y")),
                        run_command: event_time
                            .get(7)
                            .map(|s| s.to_string()) // Option<String>
                            .unwrap_or_else(|| "echo 'config error'".to_string()),
                        old_time: "not found".to_string(),
                    });
                } else if line.starts_with(":") {
                    let event_time: Vec<String> = line
                        .splitn(4, char::is_whitespace)
                        .map(|s| s.to_string())
                        .collect();

                    if event_time.len() != 4 {
                        eprintln!("{}行配置错误", line);
                        continue;
                    }
                    for (time_number, i) in event_time.iter().enumerate() {
                        if time_number == 1 {
                            for ii in i.split(":") {
                                if ii.parse::<i64>().is_err() {
                                    eprintln!(
                                        "{}行配置错误，[:] [运行时间]<- 这个配置错误 [启动时判断时间是否没晚于这个设置时间如果没有就运行命令,如果不需要就输入'n'] [运行命令]",
                                        line
                                    );
                                }
                            }
                        } else if time_number == 2 {
                            if i.to_lowercase().starts_with("n") {
                                continue;
                            }
                            for ii in i.split(":") {
                                if ii.parse::<i64>().is_err() {
                                    // 可以转成数字
                                    eprintln!(
                                        "{}行配置错误，[:] [运行时间] [启动时判断时间是否没晚于这个设置时间如果没有就运行命令,如果不需要就设置'n']<- 这个配置错误 [运行命令]",
                                        line
                                    );
                                }
                            }
                        }
                    }
                    //event_time[0] ==":" return true
                    let old_time = if event_time[2].starts_with("n") {
                        "n"
                    } else {
                        &event_time[2]
                    };
                    let run_time = &event_time[1];
                    let run_command = &event_time[3];

                    configs.push(Options {
                        interval_or_fixed: IntervalOrFixedTime::Fixed(run_time.to_string()),
                        start_run: false,
                        run_command: run_command.to_string(),
                        old_time: old_time.to_string(),
                    });
                } else {
                    continue;
                }
            }
        }
        document_modification = false;
        for run_conf in &configs {
            //判断间隔时间运行命令是否可以运行
            if let IntervalOrFixedTime::Interval(interval) = run_conf.interval_or_fixed
                && ((run_conf.start_run && time_num == 0)
                    || (time_num != 0 && time_num % interval == 0))
            {
                command(&run_conf.run_command, &sh);
            } else if let IntervalOrFixedTime::Fixed(fixed_running_time) =
                &run_conf.interval_or_fixed
                && (local_time_in(fixed_running_time) //是否到设置时间
                    || (appstart && &run_conf.old_time !="n" && old_time_compare(&run_conf.old_time,fixed_running_time)))
            //是否可以运行软件时运行命令
            {
                command(&run_conf.run_command, &sh);
            }
        }

        time_num += 1;
        sleep_time(start_time, 1);
        appstart = false
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
                    "设置的开头应以 * 或 : 开头(英文符号)如想测试示例配置因删除 # "
                )?;
                writeln!(
                    file,
                    "# 间隔时间格式为 *  是否启动软件时执行(y/n)  秒  分  时  天  月(30天)  命令"
                )?;
                writeln!(file, "# * y 3 * 1 0 2 echo 'hello world'")?;
                writeln!(
                    file,
                    "# 允许软件启动时执行，自软件启动起每隔3秒0分钟1小时0天2月(60天)执行一次命令，*和0一个意思代表空，执行echo 'hello world'"
                )?;
                writeln!(file, "# 固定时间格式为 :  天:时:分:秒(无空格)   命令")?;
                writeln!(file, "# 固定时间其他格式为 时:分:秒 / 分:秒 / 秒")?;
                writeln!(file, "# : 30:15 n echo 'Hello rust'")?;
                writeln!(
                    file,
                    "# 每个30分15秒运行echo 'Hello rust',不启用启动软件时检查条件运行命令"
                )?;
                writeln!(
                    file,
                    "# : 12:30:0 13:0:0 ~/'一个可执行文件.为了防止文件真实存在的后缀'"
                )?;
                writeln!(
                    file,
                    "# 每个12小时30分0秒运行~/'一个可执行文件.为了防止文件真实存在的后缀',如果启动软件时小于13小时00分00秒大于12小时30分00秒则强制运行软件"
                )?;

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

fn command(event_time_str: &str, sh: &str) {
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
    let count = event_time.matches(':').count();
    let local_time = match count {
        0 => Local::now().format("%S"),
        1 => Local::now().format("%M:%S"),
        2 => Local::now().format("%H:%M:%S"),
        3 => Local::now().format("%d:%H:%M:%S"),
        4 => Local::now().format("%m:%d:%H:%M:%S"),
        _ => return false,
    };
    local_time.to_string() == event_time
}

//sleep 1秒
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
//12:30:0
fn old_time_compare(old_time_start: &str, at_time_start: &str) -> bool {
    let old_time_1: Vec<&str> = old_time_start.split(":").collect();
    let at_time_1: Vec<&str> = at_time_start.split(":").collect();
    let old_time_len = old_time_1.len();
    if at_time_1.len() != old_time_len {
        eprintln!(
            "时间配置为 {} 和 {} 的行配置长度不一样无法使用启动时条件运行",
            old_time_start, at_time_start
        );
        return false;
    }
    let time_in = match old_time_len {
        1 => Local::now().format("%S").to_string(),
        2 => Local::now().format("%M:%S").to_string(),
        3 => Local::now().format("%H:%M:%S").to_string(),
        4 => Local::now().format("%d:%H:%M:%S").to_string(),
        5 => Local::now().format("%m:%d:%H:%M:%S").to_string(),
        _ => {
            eprintln!("{}格式错误", old_time_start);
            return false;
        }
    };

    let time_in: Vec<&str> = time_in.split(":").collect();
    let old_time: Vec<i32> = match old_time_1
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => v,
        Err(_) => return false,
    };

    let at_time: Vec<i32> = match at_time_1
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => v,
        Err(_) => return false,
    };

    // 解析 time_in
    let in_time: Vec<i32> = match time_in
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => v,
        Err(_) => return false,
    };
    // let mut option_ll: Vec<&str> = ["0", "0"].to_vec();
    // for i in 0..old_time_len {
    //     if option_ll[0] == "0" {
    //         if at_time[i] < in_time[i] && in_time[i] < old_time[i] {
    //             return true;
    //         } else if at_time[i] == in_time[i] {
    //             option_ll[0] = "1";
    //             option_ll[1] = "at";
    //             continue;
    //         } else if old_time[i] == in_time[i] {
    //             option_ll[0] = "1";
    //             option_ll[1] = "old";
    //             continue;
    //         }
    //     } else if option_ll[0] == "1" {
    //         if option_ll[1] == "at" {
    //             if in_time[i] >= at_time[i] {
    //                 if i + 1 == old_time_len {
    //                     return true;
    //                 } else {
    //                     continue;
    //                 }
    //             }
    //         } else if option_ll[1] == "old" {
    //             if old_time[i] > in_time[i] {
    //                 return true;
    //             } else if old_time[i] == in_time[i] {
    //                 continue;
    //             } else {
    //                 return false;
    //             }
    //         }
    //     }
    // }//我注释了我写的屎但我没有删掉我觉得保留一部分答辩才有人知道这是我写的项目
    //false
    at_time < in_time && in_time < old_time
}
