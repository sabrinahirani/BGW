use crate::sharing::Share;

#[derive(Copy, Clone, Debug)]
pub enum Message {
    InputShare(usize, Share),
    MulShare(usize, Share),
    OutputShare(usize, Share),
    Reshare(usize, Share)
}