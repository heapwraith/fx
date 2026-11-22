//! The pipe protocol must survive garbage: a client (or anything that opens
//! the pipe) can send any bytes; the reader returns an error, never panics
//! or allocates without bound (review, D-066).

use fx_index::protocol::{Request, Response};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = (self.next() % (max as u64 + 1)) as usize;
        (0..len).map(|_| self.next() as u8).collect()
    }
}

#[test]
fn requests_and_responses_survive_garbage() {
    let mut rng = Rng(0x0BAD_C0DE_F00D_0001);
    // Valid messages, damaged.
    let mut request = Vec::new();
    Request {
        query: "relatório".into(),
        limit: 50,
        ..Default::default()
    }
    .write_to(&mut request)
    .unwrap();
    for _ in 0..3000 {
        let mut damaged = request.clone();
        if !damaged.is_empty() {
            let at = (rng.next() as usize) % damaged.len();
            damaged[at] = rng.next() as u8;
        }
        damaged.truncate((rng.next() as usize) % (damaged.len() + 1));
        let _ = Request::read_from(&mut damaged.as_slice());
        let noise = rng.bytes(512);
        let _ = Request::read_from(&mut noise.as_slice());
        let _ = Response::read_from(&mut noise.as_slice());
    }
    // A length field claiming gigabytes must be refused, not allocated.
    let huge = [0xFFu8; 64];
    assert!(Request::read_from(&mut huge.as_slice()).is_err());
    assert!(Response::read_from(&mut huge.as_slice()).is_err());
}
