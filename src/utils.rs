use crate::Result;
use serde::de::DeserializeOwned;
use std::fs::{rename, File};
use std::io::{copy, BufWriter, Read};
use std::path::Path;

pub fn get_json<T>(
    client: &reqwest::blocking::Client,
    url: &str,
    query: Option<Vec<(String, String)>>,
    headers: Option<Vec<(String, String)>>,
) -> Result<T>
where
    T: DeserializeOwned,
{
    // TODO - If there's a list then support continuationToken
    let mut req = client.get(url);
    if let Some(query_params) = query {
        req = req.query(&query_params);
    }
    if let Some(headers) = headers {
        for (name, value) in headers.into_iter() {
            req = req.header(&name, value)
        }
    }
    let mut resp = req.send()?;
    resp.error_for_status_ref()?;
    let mut resp_body = match resp.content_length() {
        Some(len) => String::with_capacity(len as usize),
        None => String::new(),
    };
    resp.read_to_string(&mut resp_body)?;
    let data: T = serde_json::from_str(&resp_body)?;
    Ok(data)
}

pub fn url(base: &str, path: &str) -> String {
    format!("{}{}", base, path)
}

pub fn download(client: &reqwest::blocking::Client, name: &Path, url: &str, compress: bool) {
    let tmp_name = name.with_extension("tmp");
    let mut dest = BufWriter::new(File::create(&tmp_name).unwrap());
    let mut resp = client.get(url).send().unwrap();
    if compress {
        zstd::stream::copy_encode(&mut resp, &mut dest, 0).unwrap();
    } else {
        copy(&mut resp, &mut dest).unwrap();
    }
    rename(&tmp_name, name).unwrap();
}
