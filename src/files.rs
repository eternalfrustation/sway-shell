use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    num::ParseIntError,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum ReadIntError {
    Parsing(ParseIntError),
    StdIoError(std::io::Error),
}

impl From<ParseIntError> for ReadIntError {
    fn from(value: ParseIntError) -> Self {
        Self::Parsing(value)
    }
}

impl From<std::io::Error> for ReadIntError {
    fn from(value: std::io::Error) -> Self {
        Self::StdIoError(value)
    }
}

pub fn read_int_from_file(file: &mut File) -> Result<usize, ReadIntError> {
    Ok(read_string_from_file(file)?.trim().parse()?)
}

pub fn read_string_from_file(file: &mut File) -> Result<String, std::io::Error> {
    file.seek(SeekFrom::Start(0))?;
    let mut result_str = String::new();
    file.read_to_string(&mut result_str)?;
    Ok(result_str)
}

pub fn read_string_from_file_path<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    read_string_from_file(&mut file)
}

pub fn read_int_from_file_path<P: AsRef<Path>>(path: P) -> Result<usize, ReadIntError> {
    Ok(read_string_from_file_path(path)?.trim().parse()?)
}
