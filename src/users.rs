use std::collections::HashMap;
use std::fs;

pub fn get_system_users_map() -> HashMap<u32, String> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                let username = parts[0].to_string();
                if let Ok(uid) = parts[2].parse::<u32>() {
                    if (1000..60000).contains(&uid) && username != "nobody" {
                        map.insert(uid, username);
                    }
                }
            }
        }
    }
    map
}
