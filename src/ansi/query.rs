//! Batched terminal capability querying.

use std::io;
use std::time::{Duration, Instant};

use super::{ColorScheme, ColorType, ParsedEvent};

#[cfg(unix)]
use super::{write_query, Reader};

/// A terminal capability query issued via [`QueryBatch`].
pub trait TerminalQuery: Clone + 'static {
    /// The decoded result type.
    type Response;
    /// Returns the bytes to write to the terminal for this query.
    fn query_bytes(&self) -> Vec<u8>;
    /// Whether `event` is this query's reply.
    fn matches(&self, event: &ParsedEvent) -> bool;
    /// Decodes the matched reply (or `None` if the query went unanswered).
    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Self::Response>;
}

/// A typed handle for retrieving one query's result from [`QueryResults`].
pub struct QueryHandle<T> {
    idx: usize,
    extract: Box<dyn Fn(Option<ParsedEvent>) -> io::Result<T>>,
}

/// The replies collected by [`QueryBatch::execute`].
pub struct QueryResults {
    results: Vec<Option<ParsedEvent>>,
}

impl QueryResults {
    /// Decodes the response for `handle`.
    pub fn get<T>(&self, handle: &QueryHandle<T>) -> io::Result<T> {
        (handle.extract)(self.results[handle.idx].clone())
    }
}

/// Accumulates queries and executes them in a single terminal round-trip.
pub struct QueryBatch {
    /// The maximum time [`execute`](Self::execute) waits for replies.
    pub timeout: Duration,
    bytes: Vec<Vec<u8>>,
    matchers: Vec<Box<dyn Fn(&ParsedEvent) -> bool>>,
    results: Vec<Option<ParsedEvent>>,
}

impl Default for QueryBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryBatch {
    /// Creates an empty batch with a 2-second default timeout.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            bytes: Vec::new(),
            matchers: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Sets how long [`execute`](Self::execute) waits for replies.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Registers a query and returns a handle for its result.
    pub fn add<Q: TerminalQuery>(&mut self, query: Q) -> QueryHandle<Q::Response> {
        let idx = self.bytes.len();
        self.bytes.push(query.query_bytes());
        let matcher = query.clone();
        self.matchers.push(Box::new(move |e| matcher.matches(e)));
        self.results.push(None);
        QueryHandle {
            idx,
            extract: Box::new(move |e| query.extract(e)),
        }
    }

    /// Sends all queries and collects their replies.
    #[cfg(unix)]
    pub fn execute(self) -> io::Result<QueryResults> {
        let mut reader = Reader::for_query()?;

        let mut bytes: Vec<u8> = Vec::new();
        for segment in self.bytes {
            if segment.windows(2).any(|w| w == b"\x1b_") {
                bytes.extend_from_slice(b"\x1b[22;2t");
                bytes.extend(segment);
                bytes.extend_from_slice(b"\x1b[23;2t");
            } else {
                bytes.extend(segment);
            }
        }
        bytes.extend_from_slice(b"\x1b[c");
        write_query(&bytes)?;

        let mut results = self.results;
        let matchers = self.matchers;
        let deadline = Instant::now() + self.timeout;

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            if !reader.poll(deadline - now)? {
                continue;
            }
            let event = match reader.try_read() {
                Some(e) => e,
                None => continue,
            };
            let is_da1 = matches!(event, ParsedEvent::PrimaryDeviceAttributes(_));
            for (matcher, result) in matchers.iter().zip(results.iter_mut()) {
                if result.is_none() && matcher(&event) {
                    *result = Some(event.clone());
                }
            }
            if is_da1 {
                break;
            }
        }

        Ok(QueryResults { results })
    }
}

/// A pixel-dimension reply.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PixelSize {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
}

/// Queries the RGB value of a single terminal color slot.
#[derive(Clone)]
pub struct QueryColor(
    /// The color slot to query.
    pub ColorType,
);

impl TerminalQuery for QueryColor {
    type Response = Option<(u8, u8, u8)>;

    fn query_bytes(&self) -> Vec<u8> {
        let n = self.0.get_osc_number();
        match self.0 {
            ColorType::Indexed(index) => format!("\x1b]{n};{index};?\x1b\\").into_bytes(),
            _ => format!("\x1b]{n};?\x1b\\").into_bytes(),
        }
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::Color(entry) if entry.color_type == self.0)
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<(u8, u8, u8)>> {
        match event {
            Some(ParsedEvent::Color(entry)) => Ok(Some((entry.r, entry.g, entry.b))),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries the terminal's light/dark color scheme.
#[derive(Clone)]
pub struct QueryColorScheme;

impl TerminalQuery for QueryColorScheme {
    type Response = Option<ColorScheme>;

    fn query_bytes(&self) -> Vec<u8> {
        b"\x1b[?996n".to_vec()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::ColorScheme(_))
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<ColorScheme>> {
        match event {
            Some(ParsedEvent::ColorScheme(s)) => Ok(Some(s)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries Kitty graphics-protocol support.
#[derive(Clone)]
pub struct QueryKittyGraphicsSupport;

impl TerminalQuery for QueryKittyGraphicsSupport {
    /// `None` if unsupported, `Some(false)` for graphics-only, `Some(true)` for graphics with shared-memory.
    type Response = Option<bool>;

    fn query_bytes(&self) -> Vec<u8> {
        #[cfg(not(feature = "images"))]
        return Vec::new();
        #[cfg(feature = "images")]
        {
            #[cfg(unix)]
            if let Some(name) = crate::render::image::shm::write_probe() {
                use base64_simd::STANDARD as BASE64;
                let mut bytes = b"\x1b_Gi=31,s=1,v=1,a=q,t=s,f=24;".to_vec();
                bytes.extend_from_slice(BASE64.encode_to_string(name.as_bytes()).as_bytes());
                bytes.extend_from_slice(b"\x1b\\");
                return bytes;
            }
            b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\".to_vec()
        }
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::KittyGraphicsReply { .. })
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<bool>> {
        match event {
            Some(ParsedEvent::KittyGraphicsReply { ok, .. }) => Ok(Some(ok)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries sixel graphics support.
#[derive(Clone)]
pub struct QuerySixelSupport;

impl TerminalQuery for QuerySixelSupport {
    type Response = bool;

    fn query_bytes(&self) -> Vec<u8> {
        Vec::new()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::PrimaryDeviceAttributes(_))
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<bool> {
        match event {
            Some(ParsedEvent::PrimaryDeviceAttributes(attrs)) => Ok(attrs.contains(&4)),
            None => Ok(false),
            _ => unreachable!(),
        }
    }
}

/// Queries the terminal name and version string.
#[derive(Clone)]
pub struct QueryXtVersion;

impl TerminalQuery for QueryXtVersion {
    type Response = Option<String>;

    fn query_bytes(&self) -> Vec<u8> {
        b"\x1b[>q".to_vec()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::XtVersion(_))
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<String>> {
        match event {
            Some(ParsedEvent::XtVersion(s)) => Ok(Some(s)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries the window size in pixels.
#[derive(Clone)]
pub struct QueryWindowPixelSize;

impl TerminalQuery for QueryWindowPixelSize {
    type Response = Option<PixelSize>;

    fn query_bytes(&self) -> Vec<u8> {
        b"\x1b[14t".to_vec()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::WindowPixelSize { .. })
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<PixelSize>> {
        match event {
            Some(ParsedEvent::WindowPixelSize { width, height }) => Ok(Some(PixelSize { width, height })),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries the cell size in pixels.
#[derive(Clone)]
pub struct QueryCellPixelSize;

impl TerminalQuery for QueryCellPixelSize {
    type Response = Option<PixelSize>;

    fn query_bytes(&self) -> Vec<u8> {
        b"\x1b[16t".to_vec()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::CellPixelSize { .. })
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<PixelSize>> {
        match event {
            Some(ParsedEvent::CellPixelSize { width, height }) => Ok(Some(PixelSize { width, height })),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}

/// Queries pixel-precision mouse reporting support.
#[derive(Clone)]
pub struct QueryMousePixelMode;

impl TerminalQuery for QueryMousePixelMode {
    type Response = Option<bool>;

    fn query_bytes(&self) -> Vec<u8> {
        b"\x1b[?1016$p".to_vec()
    }

    fn matches(&self, event: &ParsedEvent) -> bool {
        matches!(event, ParsedEvent::DecModeReport { mode: 1016, .. })
    }

    fn extract(&self, event: Option<ParsedEvent>) -> io::Result<Option<bool>> {
        match event {
            Some(ParsedEvent::DecModeReport { status, .. }) => Ok(Some(status != 0)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}
