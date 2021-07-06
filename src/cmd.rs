use std::{ffi::OsString, fmt};

use clap::{App, ArgMatches};
use std::collections::HashMap;
use thiserror::Error;

/// The `Prog` struct tracks all commands and contains methods to run them
/// using an input string
#[derive(Default, Clone)]
pub struct Prog<'a, T, E: fmt::Display + fmt::Debug> {
    /// A map of command names to commands
    cmds: HashMap<String, Cmd<'a, T, E>>,
}

#[derive(Error, Debug)]
pub enum CmdErr<E: std::fmt::Display + std::fmt::Debug> {
    #[error("An error occurred when parsing arguments: {0}")]
    ArgErr(#[from] clap::Error),

    #[error("Error when splitting commands: {0}")]
    MismatchedQuotes(#[from] shellwords::MismatchedQuotes),

    #[error("A command with no contents was encountered!")]
    EmptyCommand,

    #[error("An unknown command {0} was used")]
    UnknownCommand(String),

    #[error("An error occurred when running commands: {0}")]
    RunErr(E),
}

impl<'a, T, E: fmt::Display + fmt::Debug> Prog<'a, T, E> {
    pub fn new() -> Self {
        Self {
            cmds: HashMap::new(),
        }
    }

    /// Add a command to this program
    pub fn with_cmd(mut self, cmd: Cmd<'a, T, E>) -> Self {
        self.cmds.insert(cmd.app.get_name().to_owned(), cmd);
        self
    }

    /// Run this program with the given input string
    pub fn run(&self, input: &'a str, start: T) -> Result<T, CmdErr<E>> {
        let mut val = start;
        let input = shellwords::split(input)?;
        let input = input.split(|s| s == "|");
        for command in input {
            let name = command.get(0).ok_or(CmdErr::EmptyCommand)?;
            let cmd = self
                .cmds
                .get(name)
                .ok_or_else(|| CmdErr::UnknownCommand(name.to_owned()))?;
            val = cmd.exec(val, command, self)?;
        }
        Ok(val)
    }
}

/// The `Cmd` struct defines an [App](clap::App) for argument parsing and
/// a function to run on input data
#[derive(Clone)]
pub struct Cmd<'a, T, E: fmt::Display + fmt::Debug> {
    /// The App that describes name, arguments, and subcommands
    app: App<'a, 'a>,

    /// The function to run when this command is used
    run: fn(T, ArgMatches, &Prog<'a, T, E>) -> Result<T, E>,
}

impl<'a, T, E: fmt::Display + fmt::Debug> Cmd<'a, T, E> {
    /// Create a new `Cmd` from an App and a function to run  
    pub fn new(app: App<'a, 'a>, run: fn(T, ArgMatches, &Prog<'a, T, E>) -> Result<T, E>) -> Self {
        Self { app, run }
    }

    /// Run this `Cmd` with the given argument string and value
    fn exec(
        &self,
        val: T,
        args: &[(impl Into<OsString> + Clone)],
        prog: &Prog<'a, T, E>,
    ) -> Result<T, CmdErr<E>> {
        let matches = self
            .app
            .clone()
            .get_matches_from_safe(args.into_iter().cloned())?;
        (self.run)(val, matches, prog).map_err(|e| CmdErr::RunErr(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;

    #[test]
    fn run_prog() {
        let prog: Prog<String, i32> = Prog::new()
            .with_cmd(Cmd::new(
                App::new("echo").arg(Arg::with_name("val").required(true).takes_value(true)),
                |_, args, _| Ok(args.value_of("val").unwrap().to_owned()),
            ))
            .with_cmd(Cmd::new(
                App::new("+").arg(Arg::with_name("val").required(true).takes_value(true)),
                |val, args, _| Ok(val + args.value_of("val").unwrap()),
            ))
            .with_cmd(Cmd::new(
                App::new("print"),
                |val, _, _| {
                    print!("{}", val);
                    Ok(val)
                }
            ));
        let res = prog.run("echo \"testing\" | + \" hello | world!\" | print", "".to_owned());
        assert_eq!(res.unwrap(), "testing hello | world!".to_owned());
    }
}
