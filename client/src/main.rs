use std::sync::Arc;
use async_std::{
    io::{WriteExt, BufReader, prelude::BufReadExt},
    net::{TcpStream},
    task
};
use async_std::stream::StreamExt;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};
use async_std::io;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    task::block_on(sender())
}

async fn sender() -> Result<()> {
    print_request("Chat password: ").await?;
    let mut password = String::new();
    io::stdin().read_line(&mut password).await?;
    let encryptor = new_magic_crypt!(password.trim(), 256);

    let stdin = async_std::io::stdin();
    let mut stream = TcpStream::connect("localhost:2425").await?;
    println!("Connected to server");

    print_request("Your name: ").await?;
    let mut name = String::new();
    stdin.read_line(&mut name).await?;
    stream.write(name.as_bytes()).await?;

    task::spawn(listen_chat(stream.clone()));
    
    let mut line = String::new();
    loop {
        print_request("> ").await?;
        line.clear();
        stdin.read_line(&mut line).await?;
        if line.trim() == "q" {
            break;
        }
        
        let encrypted_message = encryptor.encrypt_str_to_base64(line.trim()) + "\n";
        stream.write(encrypted_message.as_bytes()).await?;
        stream.flush().await?;
    }
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    Ok(())
}

async fn listen_chat(stream: TcpStream) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next().await {
        let line = line?;
        println!("{}", line);
    }
    Ok(())
}

async fn print_request(request: &str) -> Result<()> {
    print!("{}", request);
    Ok(io::stdout().flush().await?)
}