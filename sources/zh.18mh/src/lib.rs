#![no_std]

use aes::Aes128;
use aidoku::{
	Chapter, ContentRating, CoverImageProcessor, DeepLinkHandler, DeepLinkResult, FilterValue,
	ImageRequestProvider, ImageResponse, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, PageContext, PageImageProcessor, Result, Source,
	UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString},
	imports::{canvas::ImageRef, error::AidokuError, html::Element, net::Request},
	prelude::*,
};
use cbc::{
	Decryptor,
	cipher::{BlockDecryptMut, KeyIvInit, block_padding::NoPadding},
};

const BASE_URL: &str = "https://18mh.net";
const PAGE_TRIGGER_URL: &str = "https://18mh.net/static/frontend/assets/favicon.png";
const IMAGE_KEY: &[u8; 16] = b"f5d965df75336270";
const IMAGE_IV: &[u8; 16] = b"97b60394abc2fbe1";

struct Mh18;

impl Source for Mh18 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let url = match query {
			Some(query) if !query.trim().is_empty() => {
				format!(
					"{BASE_URL}/comic/all?key_word={}&page={}",
					encode_query(query.trim()),
					page.max(1)
				)
			}
			_ => listing_url(page),
		};
		parse_manga_page(&url)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let manga_url = absolute_url(&manga.key);
		let html = Request::get(&manga_url)?.html()?;
		if needs_details {
			manga.title = html
				.select_first(".jm-dt-title")
				.and_then(|el| el.text())
				.map(clean_text)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first(".jm-dt-cover__img")
				.and_then(|el| attr_url(&el, "data-src").or_else(|| attr_url(&el, "src")))
				.or(manga.cover);
			manga.description = html
				.select_first(".jm-dt-desc__p")
				.and_then(|el| el.text())
				.map(clean_text);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}
		if needs_chapters {
			manga.chapters = html
				.select(".jm-dt-grid a.jm-dt-ch[href^='/comic/chapter/']")
				.map(|els| {
					els.filter_map(|a| {
						let href = a.attr("href")?;
						let title = a
							.select_first(".jm-dt-ch__no")
							.and_then(|el| el.text())
							.unwrap_or_default();
						let subtitle = a
							.select_first(".jm-dt-ch__t")
							.and_then(|el| el.text())
							.unwrap_or_default();
						Some(Chapter {
							key: href.clone(),
							title: Some(clean_text(format!("{title} {subtitle}"))),
							url: Some(absolute_url(&href)),
							..Default::default()
						})
					})
					.collect()
				});
		}
		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let html = Request::get(absolute_url(&chapter.key))?.html()?;
		Ok(html
			.select(".jm-rd-pages img[data-src]")
			.map(|els| {
				els.filter_map(|img| {
					let url = attr_url(&img, "data-src")?;
					let mut context = PageContext::new();
					context.insert(String::from("url"), url.clone());
					Some(Page {
						content: PageContent::url_context(&unique_trigger_url(&url), context),
						..Default::default()
					})
				})
				.collect()
			})
			.unwrap_or_default())
	}
}

impl ListingProvider for Mh18 {
	fn get_manga_list(&self, _listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(page))
	}
}

impl DeepLinkHandler for Mh18 {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = url.strip_prefix(BASE_URL).unwrap_or(&url).to_string();
		if key.starts_with("/comic/detail/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

impl ImageRequestProvider for Mh18 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", BASE_URL))
	}
}

impl CoverImageProcessor for Mh18 {
	fn process_cover_image(&self, response: ImageResponse) -> Result<ImageRef> {
		let data = response.image.data();
		if !data.is_empty() {
			return decode_image(data);
		}
		let url = response
			.request
			.url
			.ok_or_else(|| AidokuError::message("missing 18mh cover URL"))?;
		decode_image(Request::get(url)?.header("Referer", BASE_URL).data()?)
	}
}

impl PageImageProcessor for Mh18 {
	fn process_page_image(
		&self,
		response: ImageResponse,
		context: Option<PageContext>,
	) -> Result<ImageRef> {
		let Some(url) = context.and_then(|ctx| ctx.get("url").cloned()) else {
			return Ok(response.image);
		};
		decode_image(Request::get(url)?.header("Referer", BASE_URL).data()?)
	}
}

fn decode_image(data: Vec<u8>) -> Result<ImageRef> {
	if is_plain_image(&data) {
		return Ok(ImageRef::new(&data));
	}
	if data.is_empty() || data.len() % 16 != 0 {
		return Err(AidokuError::message("invalid 18mh image data"));
	}
	let decrypted = Decryptor::<Aes128>::new(IMAGE_KEY.into(), IMAGE_IV.into())
		.decrypt_padded_vec_mut::<NoPadding>(&data)
		.map_err(|_| AidokuError::message("failed to decrypt 18mh image"))?;
	if !is_plain_image(&decrypted) {
		return Err(AidokuError::message("invalid decrypted 18mh image"));
	}
	Ok(ImageRef::new(&decrypted))
}

fn is_plain_image(data: &[u8]) -> bool {
	data.starts_with(&[0xff, 0xd8, 0xff])
		|| data.starts_with(&[0x89, 0x50, 0x4e, 0x47])
		|| data.starts_with(b"GIF87a")
		|| data.starts_with(b"GIF89a")
		|| data.get(4..12) == Some(b"ftypavif")
		|| data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP")
}

fn unique_trigger_url(url: &str) -> String {
	let mut hash = 2_166_136_261u32;
	for byte in url.as_bytes() {
		hash ^= *byte as u32;
		hash = hash.wrapping_mul(16_777_619);
	}
	format!("{PAGE_TRIGGER_URL}?aidoku={hash}")
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries: Vec<Manga> = html
		.select(".jm-card")
		.map(|els| {
			els.filter_map(|card| {
				let link = card.select_first(".jm-card__link[href^='/comic/detail/']")?;
				let href = link.attr("href")?;
				let title = link.text().map(clean_text)?;
				if title.is_empty() {
					return None;
				}
				let cover = card
					.select_first(".jm-card__img")
					.and_then(|img| attr_url(&img, "data-src"));
				Some(Manga {
					key: href.clone(),
					title,
					cover,
					content_rating: ContentRating::NSFW,
					viewer: Viewer::Webtoon,
					url: Some(absolute_url(&href)),
					..Default::default()
				})
			})
			.collect()
		})
		.unwrap_or_default();
	let has_next_page = entries.len() >= 20;
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn listing_url(page: i32) -> String {
	format!("{BASE_URL}/comic/all?page={}", page.max(1))
}

fn absolute_url(url: &str) -> String {
	if url.starts_with("http") {
		url.to_string()
	} else if url.starts_with("//") {
		format!("https:{url}")
	} else {
		format!("{BASE_URL}{url}")
	}
}

fn attr_url(el: &Element, name: &str) -> Option<String> {
	el.attr(name).map(|url| absolute_url(&url))
}

fn clean_text(text: String) -> String {
	text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn encode_query(input: &str) -> String {
	let mut output = String::new();
	for byte in input.as_bytes() {
		if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
			output.push(*byte as char);
		} else {
			output.push('%');
			output.push(hex(byte >> 4));
			output.push(hex(byte & 0x0f));
		}
	}
	output
}

fn hex(value: u8) -> char {
	match value {
		0..=9 => (b'0' + value) as char,
		_ => (b'A' + value - 10) as char,
	}
}

register_source!(
	Mh18,
	ListingProvider,
	DeepLinkHandler,
	ImageRequestProvider,
	CoverImageProcessor,
	PageImageProcessor
);
