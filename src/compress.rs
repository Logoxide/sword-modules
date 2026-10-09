//! Decompressing z* blocks by `CompressType`.

use crate::conf::CompressType;
use crate::error::{Error, Result};

/// Fail early, when a module is opened, if its compression can't be read.
pub(crate) fn check_supported(kind: &CompressType) -> Result<()> {
    match kind {
        CompressType::Lzss => Ok(()),
        CompressType::Zip if cfg!(feature = "zip") => Ok(()),
        CompressType::Bzip2 if cfg!(feature = "bzip2") => Ok(()),
        CompressType::Xz if cfg!(feature = "xz") => Ok(()),
        CompressType::Other(name) => Err(Error::UnsupportedCompression(name.clone())),
        other => Err(Error::UnsupportedCompression(format!("{other:?} (cargo feature disabled)"))),
    }
}

/// Unpack one block.
pub(crate) fn decompress(kind: &CompressType, data: &[u8]) -> Result<Vec<u8>> {
    match kind {
        CompressType::Lzss => Ok(crate::lzss::decode(data)),
        #[cfg(feature = "zip")]
        CompressType::Zip => {
            use std::io::Read;
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(data).read_to_end(&mut out).map_err(|e| Error::Decompress(format!("zlib: {e}")))?;
            Ok(out)
        }
        #[cfg(feature = "bzip2")]
        CompressType::Bzip2 => {
            use std::io::Read;
            let mut out = Vec::new();
            bzip2::read::BzDecoder::new(data).read_to_end(&mut out).map_err(|e| Error::Decompress(format!("bzip2: {e}")))?;
            Ok(out)
        }
        #[cfg(feature = "xz")]
        CompressType::Xz => {
            // A stream may be followed by zero padding; its footer never ends in a zero byte.
            let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
            let mut input = &data[..end];
            let mut out = Vec::new();
            lzma_rs::xz_decompress(&mut input, &mut out).map_err(|e| Error::Decompress(format!("xz: {e}")))?;
            Ok(out)
        }
        other => {
            check_supported(other)?;
            Err(Error::UnsupportedCompression(format!("{other:?}")))
        }
    }
}
