use crate::algorithm::encryption::Encryption;
use crate::algorithm::hash::Hash;
use crate::algorithm::mac::Mac;
use crate::error::SshError;
use crate::SshResult;
use aes::cipher::{NewCipher, StreamCipher, StreamCipherSeek};
use aes::Aes192Ctr;

const BLOCK_SIZE: usize = 24;
const IV_SIZE: usize = 16;

pub(super) struct AesCtr192 {
    pub(crate) client_key: Aes192Ctr,
    pub(crate) server_key: Aes192Ctr,

    // hmac
    mac: Box<dyn Mac>,
    ik_c_s: Vec<u8>,
    ik_s_c: Vec<u8>,
}

impl Encryption for AesCtr192 {
    fn bsize(&self) -> usize {
        BLOCK_SIZE
    }

    fn iv_size(&self) -> usize {
        IV_SIZE
    }

    fn group_size(&self) -> usize {
        16
    }

    fn new(hash: Hash, mac: Box<dyn Mac>) -> Self
    where
        Self: Sized,
    {
        let (ck, sk) = hash.mix_ek(BLOCK_SIZE);
        let mut ckey = [0u8; BLOCK_SIZE];
        let mut skey = [0u8; BLOCK_SIZE];

        let mut civ = [0u8; IV_SIZE];
        let mut siv = [0u8; IV_SIZE];

        ckey.clone_from_slice(&ck[..BLOCK_SIZE]);
        skey.clone_from_slice(&sk[..BLOCK_SIZE]);

        civ.clone_from_slice(&hash.iv_c_s[..IV_SIZE]);
        siv.clone_from_slice(&hash.iv_s_c[..IV_SIZE]);

        // TODO unwrap 未处理
        let c = Aes192Ctr::new_from_slices(&ckey, &civ).unwrap();
        let r = Aes192Ctr::new_from_slices(&skey, &siv).unwrap();

        // hmac
        let (ik_c_s, ik_s_c) = hash.mix_ik(mac.bsize());

        AesCtr192 {
            client_key: c,
            server_key: r,
            mac,
            ik_c_s,
            ik_s_c,
        }
    }

    fn encrypt(&mut self, client_sequence_num: u32, buf: &mut Vec<u8>) {
        let vec = buf.clone();
        let tag = self
            .mac
            .sign(&self.ik_c_s, client_sequence_num, vec.as_slice());
        self.client_key.apply_keystream(buf);
        buf.extend(tag.as_ref())
    }

    fn decrypt(&mut self, server_sequence_number: u32, buf: &mut [u8]) -> SshResult<Vec<u8>> {
        let pl = self.packet_len(server_sequence_number, buf);
        let data = &mut buf[..(pl + self.mac.bsize())];
        let (d, m) = data.split_at_mut(pl);
        self.server_key.apply_keystream(d);
        let tag = self.mac.sign(&self.ik_s_c, server_sequence_number, d);
        let t = tag.as_ref();
        if m != t {
            return Err(SshError::from("encryption error."));
        }
        Ok(d.to_vec())
    }

    fn packet_len(&mut self, _: u32, buf: &[u8]) -> usize {
        let bsize = self.bsize();
        let mut r = vec![0_u8; bsize];
        r.clone_from_slice(&buf[..bsize]);
        self.server_key.apply_keystream(&mut r);
        let pos: usize = self.server_key.current_pos();
        self.server_key.seek(pos - bsize);
        let mut u32_bytes = [0_u8; 4];
        u32_bytes.clone_from_slice(&r[..4]);
        let packet_len = u32::from_be_bytes(u32_bytes);
        (packet_len + 4) as usize
    }

    fn data_len(&mut self, server_sequence_number: u32, buf: &[u8]) -> usize {
        let pl = self.packet_len(server_sequence_number, buf);
        let bsize = self.mac.bsize();
        pl + bsize
    }

    fn is_cp(&self) -> bool {
        false
    }
}
