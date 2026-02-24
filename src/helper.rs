use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{Cursor, Read, Write, stdin};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use azalea_chat::FormattedText;
use base64::{engine::Engine, prelude::BASE64_STANDARD};
use flate2::{bufread::ZlibDecoder, write::ZlibEncoder};
use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use serde::Deserialize;
use serde_json::{Value, from_str};

fn read_varint(stream: &mut TcpStream) -> Result<VarInt, Box<dyn Error>> {
    // varianta clasica, doar pentru tcpstream
    let res: VarInt = stream.read_var_int()?;

    Ok(res)
}

fn read_varint_cursor(stream: &mut Cursor<Vec<u8>>) -> Result<VarInt, Box<dyn Error>> {
    // varianta pentru cursor
    let res: VarInt = stream.read_var_int()?;

    Ok(res)
}

// Never used :(

// fn read_array_dynamic(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn Error>> {
//     let result_size: i32 = i32::from(read_varint(stream)?);

//     let mut result_buf: Vec<u8> = vec![0u8; result_size as usize];
//     stream.read_exact(&mut result_buf)?;

//     Ok(result_buf)
// }

// fn read_array_fixed(stream: &mut TcpStream, buf_size: usize) -> Result<Vec<u8>, Box<dyn Error>> {
//     let mut result_buf: Vec<u8> = vec![0u8; buf_size];
//     stream.read_exact(&mut result_buf)?;

//     Ok(result_buf)
// }

fn read_array_dynamic_cursor(stream: &mut Cursor<Vec<u8>>) -> Result<Vec<u8>, Box<dyn Error>> {
    let result_size: i32 = i32::from(read_varint_cursor(stream)?);

    let mut result_buf: Vec<u8> = vec![0u8; result_size as usize];
    stream.read_exact(&mut result_buf)?;

    Ok(result_buf)
}

fn read_array_fixed_cursor(
    stream: &mut Cursor<Vec<u8>>,
    buf_size: usize,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut result_buf: Vec<u8> = vec![0u8; buf_size];
    stream.read_exact(&mut result_buf)?;

    Ok(result_buf)
}

fn send_packet_raw(stream: &mut TcpStream, data: Vec<u8>) -> Result<(), Box<dyn Error>> {
    stream.write_var_int(VarInt::from(data.len() as i32))?;
    stream.write_all(&data)?;
    Ok(())
}

fn send_packet_compressed(
    stream: &mut TcpStream,
    data: Vec<u8>,
    initial_len: i32,
) -> Result<(), Box<dyn Error>> {
    let mut final_packet: Vec<u8> = Vec::new();
    final_packet.write_var_int(VarInt::from(initial_len))?;
    final_packet.write_all(&data)?;

    stream.write_var_int(VarInt::from(final_packet.len() as i32))?;
    stream.write_all(&final_packet)?;
    Ok(())
}

fn send_packet(
    stream: &mut TcpStream,
    packet_id: i32,
    data: Vec<u8>,
    threshold: i32,
) -> Result<(), Box<dyn Error>> {
    let mut raw_packet: Vec<u8> = Vec::new();
    raw_packet.write_var_int(VarInt::from(packet_id))?;
    raw_packet.write_all(&data)?;

    if threshold < 0 {
        // Compression disabled
        send_packet_raw(stream, raw_packet)?;
        return Ok(());
    }

    // Compression enabled

    if raw_packet.len() < threshold as usize {
        // Packet too small
        send_packet_compressed(stream, raw_packet, 0)?;
        Ok(())
    } else {
        // Packet big enough to be compressed
        let mut encoder: ZlibEncoder<Vec<u8>> =
            ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw_packet)?;
        let compressed_data = encoder.finish()?;

        send_packet_compressed(stream, compressed_data, raw_packet.len() as i32)?;
        Ok(())
    }
}

fn read_packet(
    cursor: &mut Cursor<Vec<u8>>,
    packet_length: i32,
) -> Result<(i32, Vec<u8>), Box<dyn Error>> {
    let packet_id: i32 = i32::from(read_varint_cursor(cursor)?);
    let curr_size: usize = (packet_length as u64 - cursor.position()) as usize;
    let data: Vec<u8> = read_array_fixed_cursor(cursor, curr_size)?;

    Ok((packet_id, data))
}

fn receive_packet(
    stream: &mut TcpStream,
    threshold: i32,
) -> Result<(i32, Vec<u8>), Box<dyn Error>> {
    let packet_length: i32 = i32::from(read_varint(stream)?);
    let mut buffer: Vec<u8> = vec![0u8; packet_length as usize];
    stream.read_exact(&mut buffer)?;
    let mut cursor: Cursor<Vec<u8>> = Cursor::new(buffer);

    if threshold < 0 {
        return read_packet(&mut cursor, packet_length);
    }

    let data_length: i32 = i32::from(read_varint_cursor(&mut cursor)?);

    if data_length == 0 {
        read_packet(&mut cursor, packet_length)
    } else {
        let mut decoder: ZlibDecoder<Cursor<Vec<u8>>> = ZlibDecoder::new(cursor);
        let mut decompressed_data: Vec<u8> = Vec::new();
        decoder.read_to_end(&mut decompressed_data)?;

        let mut data_cursor: Cursor<Vec<u8>> = Cursor::new(decompressed_data);
        read_packet(&mut data_cursor, data_length)
    }
}

fn create_players_string(players: &HashMap<u128, String>) -> String {
    let mut res: String = String::new();
    res += "Online Players (";
    res += &players.keys().count().to_string();
    res += "): [";

    let mut is_first: bool = true;
    for player in players {
        if !is_first {
            res += ", ";
        }
        res += player.1;
        is_first = false;
    }
    res += "]";
    res
}

pub fn init_connection(ip: &str, port: u16) -> Result<TcpStream, Box<dyn Error>> {
    let mut attempt: u16 = 1;
    while attempt < 6 {
        println!("Attempting to connect to {}:{}! ({})", ip, port, attempt);
        match TcpStream::connect(format!("{}:{}", ip, port)) {
            Ok(stream) => {
                println!("Connected!");
                return Ok(stream);
            }
            Err(_) => {
                // sleep(Duration::from_secs(1)); // connect already hangs for ~3 seconds on fail
                attempt += 1;
            }
        }
    }
    Err("Couldn't connect to the server in 5 attempts!".into())
}

pub fn request_status(ip: &str, port: u16) -> Result<(), Box<dyn Error>> {
    println!("Requesting status from server {}:{}!", ip, port);
    let mut temp_connection: TcpStream = init_connection(ip, port)?;

    send_handshake_packet(&mut temp_connection, ip, port, 1)?;

    send_status_request(&mut temp_connection)?;

    let response_json: Value = from_str(receive_status_response(&mut temp_connection)?.as_str())?;

    let Some(status) = response_json["description"]["text"].as_str() else {
        return Err("Error while converting status string".into());
    };

    println!("Server status: {}", status);

    let Some(favicon_string) = response_json["favicon"].as_str() else {
        println!("The server does not have a server-icon!");
        return Ok(());
    };

    let Some(comma_pos) = favicon_string.find(',') else {
        return Err("The string does not contain a comma!".into());
    };

    let favicon_b64: Vec<u8> = BASE64_STANDARD.decode(&favicon_string[(comma_pos + 1)..])?;

    fs::write("server-icon.png", favicon_b64)?;
    println!("The server image has been saved to server-icon.png!");

    temp_connection.shutdown(Shutdown::Both)?;

    Ok(())
}

fn send_handshake_packet(
    stream: &mut TcpStream,
    ip: &str,
    port: u16,
    intent: i32,
) -> Result<(), Box<dyn Error>> {
    let mut packet_buffer: Vec<u8> = Vec::new();

    packet_buffer.write_var_int(VarInt::from(754))?; // protocol version

    packet_buffer.write_var_int(VarInt::from(ip.len() as i32))?;
    packet_buffer.write_all(ip.as_bytes())?;

    packet_buffer.write_all(&port.to_be_bytes())?;

    packet_buffer.write_var_int(VarInt::from(intent))?; // intent

    send_packet(stream, 0x00, packet_buffer, -1)?; // Handshake packet

    Ok(())
}

fn send_status_request(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let packet_buffer: Vec<u8> = Vec::new();

    send_packet(stream, 0x00, packet_buffer, -1)?; // Status Request packet

    Ok(())
}

fn receive_status_response(stream: &mut TcpStream) -> Result<String, Box<dyn Error>> {
    let packet: (i32, Vec<u8>) = receive_packet(stream, -1)?; // Status Response packet

    let mut buf: Cursor<Vec<u8>> = Cursor::new(packet.1);
    let packet_size: i32 = i32::from(read_varint_cursor(&mut buf)?);
    let packet_data: Vec<u8> = read_array_fixed_cursor(&mut buf, packet_size as usize)?;

    Ok(String::from_utf8(packet_data)?) //
}

fn send_keep_alive_packet(
    stream: &Arc<Mutex<TcpStream>>,
    cursor: &mut Cursor<Vec<u8>>,
    threshold: i32,
) -> Result<(), Box<dyn Error>> {
    let packet_secret: Vec<u8> = read_array_fixed_cursor(cursor, 8)?;
    let mut new_packet_buffer: Vec<u8> = Vec::new();

    new_packet_buffer.write_all(&packet_secret).unwrap();

    let mut guard = stream.lock().unwrap();
    send_packet(&mut guard, 0x10, new_packet_buffer, threshold)?;
    Ok(())
}

fn receive_chat_message(cursor: &mut Cursor<Vec<u8>>) -> Result<(), Box<dyn Error>> {
    let response_buf: Vec<u8> = read_array_dynamic_cursor(cursor)?;

    let chat_message: String = String::from_utf8(response_buf)?;
    let json_str: Value = serde_json::from_str(chat_message.as_str())?;
    let text: FormattedText = FormattedText::deserialize(&json_str)?;

    println!("{}", text.to_ansi());

    Ok(())
}

fn create_player_list(
    cursor: &mut Cursor<Vec<u8>>,
    online_players: &Arc<Mutex<HashMap<u128, String>>>,
) -> Result<(), Box<dyn Error>> {
    let action: i32 = i32::from(read_varint_cursor(cursor)?);
    let number_of_players: i32 = i32::from(read_varint_cursor(cursor)?);
    let mut players: MutexGuard<'_, HashMap<u128, String>> = online_players.lock().unwrap();

    for _ in 0..number_of_players {
        let uuid_arr: Vec<u8> = read_array_fixed_cursor(cursor, 16)?;
        let uuid: u128 = u128::from_be_bytes(
            uuid_arr
                .try_into()
                .expect("Vector doesn't have 16 characters!"),
        );
        if action == 0 {
            let name: String = String::from_utf8(read_array_dynamic_cursor(cursor)?)?;
            players.entry(uuid).or_insert(name);

            let number_of_properties = i32::from(read_varint_cursor(cursor)?);
            for _ in 0..number_of_properties {
                let _ = read_array_dynamic_cursor(cursor)?; // name
                let _ = read_array_dynamic_cursor(cursor)?; // value
                let is_signed = read_array_fixed_cursor(cursor, 1)?;
                if is_signed[0] == 1 {
                    let _ = read_array_dynamic_cursor(cursor)?;
                }
            }
            let _ = read_varint_cursor(cursor)?;
            let _ = read_varint_cursor(cursor)?;
            let has_disply_name = read_array_fixed_cursor(cursor, 1)?;
            if has_disply_name[0] == 1 {
                let _ = read_array_dynamic_cursor(cursor)?;
            }
        }
        if action == 1 || action == 2 {
            let _ = read_varint_cursor(cursor)?;
        }
        if action == 3 {
            let has_disply_name = read_array_fixed_cursor(cursor, 1)?;
            if has_disply_name[0] == 1 {
                let _ = read_array_dynamic_cursor(cursor)?;
            }
        }
        if action == 4 {
            players.remove(&uuid);
        }
    }
    Ok(())
}

pub fn start(ip: &str, port: u16, username: &str) -> Result<(), Box<dyn Error>> {
    let mut stream: TcpStream = init_connection(ip, port)?;
    let online_players: Arc<Mutex<HashMap<u128, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let online_players_clone: Arc<Mutex<HashMap<u128, String>>> = Arc::clone(&online_players);
    let mut threshold: i32 = -1;

    send_handshake_packet(&mut stream, ip, port, 2)?; // C -> S: Handshake

    let mut packet_buffer: Vec<u8> = Vec::new();

    packet_buffer.write_var_int(VarInt::from(username.len() as i32))?;
    packet_buffer.write_all(username.as_bytes())?;

    send_packet(&mut stream, 0x00, packet_buffer, threshold)?; // Login Start packet

    let packet: (i32, Vec<u8>) = receive_packet(&mut stream, -1)?;
    if packet.0 == 0x03 {
        let mut cursor: Cursor<Vec<u8>> = Cursor::new(packet.1);
        threshold = i32::from(read_varint_cursor(&mut cursor)?);
        println!(
            "Compression packet received (new threshold: {}), compressing all packets...",
            threshold
        );
    }
    let threshold_clone: i32 = threshold;

    let shared_stream: Arc<Mutex<TcpStream>> = Arc::new(Mutex::new(
        stream.try_clone().expect("Failed to copy stream."),
    ));
    let shared_stream_clone: Arc<Mutex<TcpStream>> = Arc::clone(&shared_stream);

    thread::spawn(move || {
        let mut buffer: String = String::new();

        loop {
            if stdin().read_line(&mut buffer).is_err() {
                panic!("Error while reading from terminal!");
            }

            buffer = String::from(buffer.trim());

            if buffer.len() > 255 {
                println!("[MClient] The message can't be longer than 255 characters!");
                buffer.clear();
                continue;
            }

            if buffer.eq_ignore_ascii_case(".list") {
                let players: MutexGuard<'_, HashMap<u128, String>> =
                    online_players_clone.lock().unwrap();
                println!("[MClient] {}", create_players_string(&players));
                buffer.clear();
                continue;
            }

            if buffer.eq_ignore_ascii_case(".quit") {
                std::process::exit(0);
            }

            let mut packet_buffer: Vec<u8> = Vec::new();
            packet_buffer
                .write_var_int(VarInt::from(buffer.len() as i32))
                .unwrap();
            packet_buffer.write_all(buffer.as_bytes()).unwrap();

            {
                let mut guard = shared_stream_clone.lock().unwrap();
                send_packet(&mut guard, 0x03, packet_buffer, threshold_clone).unwrap();
            }

            buffer.clear();
        }
    });

    loop {
        let loop_packet: (i32, Vec<u8>) = receive_packet(&mut stream, threshold)?;

        let mut cursor: Cursor<Vec<u8>> = Cursor::new(loop_packet.1);

        match loop_packet.0 {
            0x1F => {
                // Keep alive packet
                send_keep_alive_packet(&shared_stream, &mut cursor, threshold)?;
            }
            0x0E => {
                // Receive chat message packet
                receive_chat_message(&mut cursor)?;
            }
            0x32 => {
                // Create list
                create_player_list(&mut cursor, &online_players)?;
            }
            _ => {
                // ignore other packets
            }
        }
    }
}
