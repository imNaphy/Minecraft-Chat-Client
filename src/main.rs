mod helper;

static IP: &str = "127.0.0.1";
static PORT: u16 = 25565;
static USERNAME: &str = "Tester12";

fn main() {
    if let Err(e) = helper::request_status(IP, PORT) {
        panic!("Error while requesting status: {}", e);
    }

    if let Err(e) = helper::start(IP, PORT, USERNAME) {
        panic!("Error while sending handshake packet: {}", e);
    }
}
