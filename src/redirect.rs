use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

#[derive(Debug)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub enum OutDest {
    Inherit,
    File { append: bool, path: String },
    SameAs(Stream),
}

impl OutDest {
    pub fn open_file(&self) -> Result<Option<File>, io::Error> {
        match self {
            OutDest::Inherit | OutDest::SameAs(_) => Ok(None),
            OutDest::File { append, path } => {
                if let Some(parent) = Path::new(path).parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)?;
                }
                let f: File = if *append {
                    OpenOptions::new().append(true).create(true).open(path)?
                } else {
                    File::create(path)?
                };
                Ok(Some(f))
            }
        }
    }
}

#[derive(Debug)]
pub struct Redirects {
    pub stdout: OutDest,
    pub stderr: OutDest,
}

impl Redirects {
    pub fn none() -> Self {
        Redirects {
            stdout: OutDest::Inherit,
            stderr: OutDest::Inherit,
        }
    }

    pub fn open_stdio(&self) -> io::Result<(Stdio, Stdio)> {
        match (&self.stdout, &self.stderr) {
            (OutDest::SameAs(Stream::Stderr), OutDest::SameAs(Stream::Stdout)) => {
                Ok((Stdio::inherit(), Stdio::inherit()))
            }
            (OutDest::SameAs(Stream::Stderr), _) => {
                let err_file = self.stderr.open_file()?;
                let out = match &err_file {
                    Some(f) => Stdio::from(f.try_clone()?),
                    None => Stdio::inherit(),
                };
                let err = match err_file {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
            (_, OutDest::SameAs(Stream::Stdout)) => {
                let out_file = self.stdout.open_file()?;
                let err = match &out_file {
                    Some(f) => Stdio::from(f.try_clone()?),
                    None => Stdio::inherit(),
                };
                let out = match out_file {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
            _ => {
                let out = match self.stdout.open_file()? {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                let err = match self.stderr.open_file()? {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
        }
    }

    pub fn open_stdout_write(&self) -> io::Result<Box<dyn Write>> {
        match &self.stdout {
            OutDest::SameAs(Stream::Stderr) => Ok(Box::new(io::stderr())),
            _ => match self.stdout.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stdout())),
            },
        }
    }

    pub fn open_stderr_write(&self) -> io::Result<Box<dyn Write>> {
        match &self.stderr {
            OutDest::SameAs(Stream::Stdout) => match self.stdout.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stdout())),
            },
            _ => match self.stderr.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stderr())),
            },
        }
    }
}

pub fn split_redirect<'a>(args: &[&'a str]) -> (Vec<&'a str>, Redirects) {
    let mut cmd_args = Vec::new();
    let mut redir = Redirects::none();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            ">" | "1>" | ">>" | "1>>" => {
                let append = matches!(args[i], ">>" | "1>>");
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stdout = OutDest::File { append, path };
                i += 2;
            }
            "2>" | "2>>" => {
                let append = args[i] == "2>>";
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stderr = OutDest::File { append, path };
                i += 2;
            }
            "&>" | "&>>" => {
                let append = args[i] == "&>>";
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stdout = OutDest::File { append, path };
                redir.stderr = OutDest::SameAs(Stream::Stdout);
                i += 2;
            }
            "2>&1" => {
                redir.stderr = OutDest::SameAs(Stream::Stdout);
                i += 1;
            }
            "1>&2" | ">&2" => {
                redir.stdout = OutDest::SameAs(Stream::Stderr);
                i += 1;
            }
            _ => {
                cmd_args.push(args[i]);
                i += 1;
            }
        }
    }
    (cmd_args, redir)
}
