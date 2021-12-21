use std::str::FromStr;

pub struct Command<'a> {
    parts: Vec<&'a str>,
}

impl<'a> Command<'a> {
    pub fn parse(line: &'a str) -> Command<'a> {
        let parts = line[1..].split(" ").collect::<Vec<&str>>();
        Command { parts }
    }

    pub fn name(&self) -> &'a str {
        self.parts[0]
    }

    pub fn arg<T: FromStr>(&self, idx: usize) -> Result<T, String> {
        let arg_no = idx + 1;
        if arg_no >= self.parts.len() {
            return Err(format!("Missing argument {}", arg_no));
        }

        match self.parts[arg_no].parse::<T>() {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("Argument {} is not valid", arg_no)),
        }
    }
}
