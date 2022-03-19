use async_std::{
    io::{BufReader, prelude::BufReadExt},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    prelude::*,
    task,
};
use async_std::io;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
use std::{
    collections::hash_map::{HashMap, Entry},
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Sender<T> = mpsc::UnboundedSender<T>;
type Receiver<T> = mpsc::UnboundedReceiver<T>;

fn main() -> Result<()> {
    let fut = accept_loop("127.0.0.1:2425");
    Ok(task::block_on(spawn_and_log_error(fut)))
}

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    print_request("Chat password: ").await?;
    let mut password = String::new();
    io::stdin().read_line(&mut password).await?;
    let encryptor = new_magic_crypt!(password.trim(), 256);

    let listener = TcpListener::bind(addr).await?;
    let (broker_sender, broker_receiver) = mpsc::unbounded();
    let _broker_handle = task::spawn(broker_loop(broker_receiver));

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("Accepting from: {}", stream.peer_addr()?);
        spawn_and_log_error(connection_loop(stream, broker_sender.clone(), encryptor.clone()));
    }
    Ok(())
}


async fn connection_loop(stream: TcpStream, mut broker: Sender<Event>, encryptor: MagicCrypt256) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    let name = match lines.next().await {
        None => Err("peer disconnected immediately")?,
        Some(line) => line?,
    };

    broker.send(Event::NewPeer { name: name.clone(), stream: Arc::clone(&stream) }).await?;
    while let Some(line) = lines.next().await {
        let line = encryptor.decrypt_base64_to_string(line?);
        if line.is_err() {
            println!("{} used invalid password", name);
            return Ok(());
        }
        let line = line.unwrap();

        println!("{}: {}", name, line);
        broker.send(Event::Message { from: name.clone(), msg: line }).await?;
    }
    Ok(())
}

async fn print_request(request: &str) -> Result<()> {
    print!("{}", request);
    Ok(io::stdout().flush().await?)
}

fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            eprintln!("{}", e)
        }
    })
}

async fn connection_writer_loop(
    mut messages: Receiver<String>,
    stream: Arc<TcpStream>,
) -> Result<()> {
    let mut stream = &*stream;
    while let Some(msg) = messages.next().await {
        stream.write_all(msg.as_bytes()).await?;
    }
    Ok(())
}

#[derive(Debug)]
enum Event {
    NewPeer {
        name: String,
        stream: Arc<TcpStream>,
    },
    Message {
        from: String,
        msg: String,
    },
}

async fn broker_loop(mut events: Receiver<Event>) -> Result<()> {
    let mut peers: HashMap<String, Sender<String>> = HashMap::new();

    while let Some(event) = events.next().await {
        match event {
            Event::Message { from, msg } => {
                for (_name, mut peer) in peers.iter() {
                    let msg = format!("{}: {}\n", from, msg);
                    peer.send(msg).await?;
                }
            }
            Event::NewPeer { name, stream} => {
                match peers.entry(name) {
                    Entry::Occupied(..) => (),
                    Entry::Vacant(entry) => {
                        let (client_sender, client_receiver) = mpsc::unbounded();
                        entry.insert(client_sender);
                        spawn_and_log_error(connection_writer_loop(client_receiver, stream));
                    }
                }
            }
        }
    }
    Ok(())
}