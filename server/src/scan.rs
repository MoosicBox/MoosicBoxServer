use audiotags::{AudioTag, Tag};
use moosicbox_core::{
    app::AppState,
    slim::player::Track,
    sqlite::db::{
        add_album_and_get_album, add_album_map_and_get_album, add_artist_map_and_get_artist,
        add_tracks, DbError, InsertTrack, SqliteValue,
    },
};
use regex::Regex;
use std::{
    collections::HashMap,
    fs::{self},
    io::Write,
    num::ParseIntError,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Tag(#[from] audiotags::error::Error),
}

pub fn scan(directory: &str, data: &AppState) -> Result<(), ScanError> {
    scan_dir(Path::new(directory).to_path_buf(), &|p| {
        create_track(p, data)
    })
}

fn save_bytes_to_file(bytes: &[u8], path: &PathBuf) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    let _ = file.write_all(bytes);
}

fn search_for_artwork(path: PathBuf, tag: Box<dyn AudioTag>) -> Option<PathBuf> {
    if let Some(cover_file) = fs::read_dir(path.clone())
        .unwrap()
        .filter_map(|p| p.ok())
        .find(|p| {
            let name = p.file_name().to_str().unwrap().to_lowercase();
            name.starts_with("cover.")
        })
        .map(|dir| dir.path())
    {
        Some(cover_file)
    } else if let Some(tag_cover) = tag.album_cover() {
        let cover_file_path = match tag_cover.mime_type {
            audiotags::MimeType::Png => path.join("cover.png"),
            audiotags::MimeType::Jpeg => path.join("cover.jpg"),
            audiotags::MimeType::Tiff => path.join("cover.tiff"),
            audiotags::MimeType::Bmp => path.join("cover.bmp"),
            audiotags::MimeType::Gif => path.join("cover.gif"),
        };
        save_bytes_to_file(tag_cover.data, &cover_file_path);
        Some(cover_file_path)
    } else {
        None
    }
}

fn create_track(path: PathBuf, data: &AppState) -> Result<(), ScanError> {
    let tag = Tag::new().read_from_path(path.to_str().unwrap())?;

    let duration = if path.to_str().unwrap().ends_with(".mp3") {
        mp3_duration::from_path(path.as_path())
            .unwrap()
            .as_secs_f64()
    } else {
        tag.duration().unwrap()
    };

    let title = tag.title().unwrap().to_string();
    let number = tag.track_number().unwrap_or(1) as i32;
    let album = tag.album_title().unwrap_or("(none)").to_string();
    let artist_name = tag.artist().or(tag.album_artist()).unwrap().to_string();
    let album_artist = tag
        .album_artist()
        .unwrap_or(artist_name.as_str())
        .to_string();
    let date_released = tag.date().map(|date| date.to_string());

    let multi_artist_pattern = Regex::new(r"\S,\S").unwrap();

    let path_artist = path.clone().parent().unwrap().parent().unwrap().to_owned();
    let artist_dir_name = path_artist
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let path_album = path.clone().parent().unwrap().to_owned();
    let album_dir_name = path_album
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    println!("====== {} ======", path.clone().to_str().unwrap());
    println!("title: {}", title);
    println!("number: {}", number);
    println!("duration: {}", duration);
    println!("album title: {}", album);
    println!("artist directory name: {}", artist_dir_name);
    println!("album directory name: {}", album_dir_name);
    println!("artist: {}", artist_name.clone());
    println!("album_artist: {}", album_artist.clone());
    println!("date_released: {:?}", date_released);
    println!("contains cover: {:?}", tag.album_cover().is_some());

    let album_artist = match multi_artist_pattern.find(album_artist.as_str()) {
        Some(comma) => album_artist[..comma.start() + 1].to_string(),
        None => album_artist,
    };

    let artist = add_artist_map_and_get_artist(
        data.db.as_ref().unwrap(),
        HashMap::from([("title", SqliteValue::String(album_artist))]),
    )?;

    let mut album = add_album_map_and_get_album(
        data.db.as_ref().unwrap(),
        HashMap::from([
            ("title", SqliteValue::String(album)),
            ("artist_id", SqliteValue::Number(artist.id as i64)),
            ("date_released", SqliteValue::StringOpt(date_released)),
            (
                "directory",
                SqliteValue::StringOpt(path_album.to_str().map(|p| p.to_string())),
            ),
        ]),
    )?;

    println!("artwork: {:?}", album.artwork);

    if album.artwork.is_none() {
        if let Some(artwork) = search_for_artwork(path_album.clone(), tag) {
            album.artwork = Some(artwork.file_name().unwrap().to_str().unwrap().to_string());
            println!(
                "Found artwork for {}: {}",
                path_album.to_str().unwrap(),
                album.artwork.clone().unwrap()
            );
            album = add_album_and_get_album(data.db.as_ref().unwrap(), album)?;
        }
    }

    let _track_id = add_tracks(
        data.db.as_ref().unwrap(),
        vec![InsertTrack {
            album_id: album.id,
            file: path.to_str().unwrap().to_string(),
            track: Track {
                number,
                title,
                duration,
                ..Default::default()
            },
        }],
    );

    Ok(())
}

fn scan_dir<F>(path: PathBuf, fun: &F) -> Result<(), ScanError>
where
    F: Fn(PathBuf) -> Result<(), ScanError>,
{
    let music_file_pattern = Regex::new(r".+\.(flac|m4a|mp3)").unwrap();

    for p in fs::read_dir(path).unwrap().filter_map(|p| p.ok()) {
        let metadata = p.metadata().unwrap();

        if metadata.is_dir() {
            scan_dir(p.path(), fun)?;
        } else if metadata.is_file()
            && music_file_pattern.is_match(p.path().file_name().unwrap().to_str().unwrap())
        {
            fun(p.path())?;
        }
    }

    Ok(())
}
