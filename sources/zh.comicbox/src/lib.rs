#![no_std]

use aes::Aes128;
use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, Home, HomeComponent,
	HomeComponentValue, HomeLayout, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, PageImageProcessor, Result,
	Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString, vec},
	imports::{
		canvas::{Canvas, ImageRef, Rect},
		error::AidokuError,
		html::Element,
		net::Request,
	},
	prelude::*,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use cbc::{
	Decryptor,
	cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7},
};
use md5::{Digest, Md5};

const BASE_URL: &str = "https://www.comicbox.xyz";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";
const CACHE_KEY: &str = "2026021808";
const PAGE_TRIGGER_URL: &str = "https://www.comicbox.xyz/static/images/logo.png";

struct ComicBox;

impl Source for ComicBox {
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
				format!("{BASE_URL}/search?keyword={}", encode_query(query.trim()))
			}
			_ => listing_url("home", page),
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
		let html = Request::get(&manga_url)?.header("User-Agent", UA).html()?;

		if needs_details {
			manga.title = html
				.select_first(".sp-book-title")
				.and_then(|el| el.text())
				.or_else(|| {
					html.select_first("meta[property='og:title']")
						.and_then(|el| el.attr("content"))
				})
				.map(clean_text)
				.map(strip_site_title)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first(".sp-book-cover .cropped[data-src]")
				.and_then(|el| attr_url(&el, "data-src"))
				.and_then(|url| recover_data_url(&url).or(Some(url)))
				.or_else(|| {
					html.select_first("meta[property='og:image']")
						.and_then(|el| attr_url(&el, "content"))
				})
				.or(manga.cover);
			manga.description = html
				.select_first(".sp-book-summary")
				.and_then(|el| el.text())
				.map(clean_text)
				.or_else(|| {
					html.select_first("meta[name='description']")
						.and_then(|el| el.attr("content"))
						.map(clean_text)
				});
			manga.tags = html.select(".sp-book-tags a").map(|els| {
				els.filter_map(|el| el.text().map(clean_text))
					.filter(|tag| !tag.is_empty())
					.collect()
			});
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}

		if needs_chapters {
			manga.chapters = html
				.select("a.sp-chapter-item[href^='/free-chapter/']")
				.map(|els| {
					let mut keys = Vec::<String>::new();
					els.filter_map(|a| {
						let href = a.attr("href")?;
						let key = normalize_key(&href);
						if keys.contains(&key) {
							return None;
						}
						keys.push(key.clone());
						Some(Chapter {
							key,
							title: a
								.attr("title")
								.or_else(|| a.text())
								.map(clean_text)
								.filter(|title| !title.is_empty()),
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
		let html = Request::get(absolute_url(&chapter.key))?
			.header("User-Agent", UA)
			.html()?;
		Ok(html
			.select(".comicpage .cropped[data-src], .comiclist .cropped[data-src]")
			.map(|els| {
				let mut urls = Vec::<String>::new();
				els.filter_map(|el| {
					let url = attr_url(&el, "data-src")?;
					if !is_image_url(&url) || urls.contains(&url) {
						return None;
					}
					urls.push(url.clone());
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

impl ListingProvider for ComicBox {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl Home for ComicBox {
	fn get_home(&self) -> Result<HomeLayout> {
		Ok(HomeLayout {
			components: vec![
				manga_list_component("首页", "home", false)?,
				manga_list_component("分类", "all", false)?,
			],
		})
	}
}

impl DeepLinkHandler for ComicBox {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if key.starts_with("/free-chapter/") {
			Ok(Some(DeepLinkResult::Chapter {
				manga_key: String::new(),
				key,
			}))
		} else if key.starts_with("/book/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.header("User-Agent", UA).html()?;
	let entries = html
		.select("a[href^='/book/']")
		.map(|els| {
			let mut keys = Vec::<String>::new();
			els.filter_map(|a| {
				let href = a.attr("href")?;
				let key = normalize_key(&href);
				if keys.contains(&key) {
					return None;
				}
				let title = a
					.attr("title")
					.or_else(|| {
						a.select_first(".sp-card-title, .title, h3")
							.and_then(|el| el.text())
					})
					.or_else(|| a.text())
					.map(clean_text)?;
				if title.is_empty() {
					return None;
				}
				keys.push(key.clone());
				Some(Manga {
					key,
					title,
					cover: a
						.select_first(".cropped[data-src]")
						.and_then(|el| attr_url(&el, "data-src"))
						.and_then(|url| recover_data_url(&url).or(Some(url)))
						.or_else(|| {
							a.select_first("img").and_then(|img| {
								attr_url(&img, "data-src").or_else(|| attr_url(&img, "src"))
							})
						}),
					content_rating: ContentRating::NSFW,
					viewer: Viewer::Webtoon,
					url: Some(absolute_url(&href)),
					..Default::default()
				})
			})
			.collect()
		})
		.unwrap_or_default();
	Ok(MangaPageResult {
		entries,
		has_next_page: true,
	})
}

fn listing_url(id: &str, page: i32) -> String {
	let path = match id {
		"all" => "/booklist",
		_ => "/index",
	};
	if page <= 1 {
		format!("{BASE_URL}{path}")
	} else if path.contains('?') {
		format!("{BASE_URL}{path}&page={page}")
	} else {
		format!("{BASE_URL}{path}?page={page}")
	}
}

fn manga_list_component(title: &str, id: &str, ranking: bool) -> Result<HomeComponent> {
	Ok(HomeComponent {
		title: Some(title.into()),
		subtitle: None,
		value: HomeComponentValue::MangaList {
			ranking,
			page_size: Some(12),
			entries: parse_manga_page(&listing_url(id, 1))?
				.entries
				.into_iter()
				.map(|manga| manga.into())
				.collect(),
			listing: None,
		},
	})
}

fn normalize_key(url: &str) -> String {
	url.strip_prefix(BASE_URL).unwrap_or(url).to_string()
}

fn absolute_url(url: &str) -> String {
	if url.starts_with("http://") || url.starts_with("https://") {
		url.into()
	} else if url.starts_with("//") {
		format!("https:{url}")
	} else {
		format!("{BASE_URL}{url}")
	}
}

fn attr_url(el: &Element, name: &str) -> Option<String> {
	el.attr(name).map(|url| absolute_url(&url))
}

fn recover_image(url: &str) -> Result<ImageRef> {
	let (data, book_id, page_number) = recover_image_data(url)?;
	let image = ImageRef::new(&data);
	if book_id > 0 {
		Ok(merge_manga_image(image, book_id, page_number))
	} else {
		Ok(image)
	}
}

fn recover_data_url(url: &str) -> Option<String> {
	let image = recover_image(url).ok()?;
	let data = image.data();
	let mime = match data.get(0..4) {
		Some([0xff, 0xd8, 0xff, _]) => "image/jpeg",
		Some([0x89, 0x50, 0x4e, 0x47]) => "image/png",
		Some([0x47, 0x49, 0x46, _]) => "image/gif",
		_ => "image/jpeg",
	};
	Some(format!("data:{mime};base64,{}", STANDARD.encode(data)))
}

fn recover_image_data(url: &str) -> Result<(Vec<u8>, i32, i32)> {
	let split_urls = split_image_urls(url);
	let mut data = Vec::<u8>::new();

	for url in split_urls {
		let encrypted = Request::get(&url)?
			.header("User-Agent", UA)
			.header("Referer", BASE_URL)
			.data()?;
		let mut decrypted = decrypt_bytes(encrypted)?;
		data.append(&mut decrypted);
	}

	let (book_id, page_number) = parse_magic_info(&data)?;
	inject_magic_number(&mut data)?;
	Ok((data, book_id, page_number))
}

fn split_image_urls(url: &str) -> Vec<String> {
	let clean = strip_query(url)
		.replace("/break_2/", "/")
		.replace("/break_avif/", "/");
	let with_path = insert_break_path(&clean);
	vec![
		replace_extension(&with_path, ".b_0"),
		replace_extension(&with_path, ".b_1"),
	]
	.into_iter()
	.map(|url| format!("{url}?v={CACHE_KEY}"))
	.collect()
}

fn strip_query(url: &str) -> &str {
	url.split('?').next().unwrap_or(url)
}

fn insert_break_path(url: &str) -> String {
	if let Some(index) = url[8..].find('/') {
		let split = index + 8;
		format!("{}/break_2{}", &url[..split], &url[split..])
	} else {
		url.to_string()
	}
}

fn replace_extension(url: &str, replacement: &str) -> String {
	for ext in [".jpeg", ".jpg", ".png", ".gif", ".avif", ".webp"] {
		if url.to_ascii_lowercase().ends_with(ext) {
			return format!("{}{}", &url[..url.len() - ext.len()], replacement);
		}
	}
	url.to_string()
}

fn decrypt_bytes(data: Vec<u8>) -> Result<Vec<u8>> {
	let key = *b"aaaaaaaaaaaaaaaa";
	let iv = *b"0123456789aaaaaa";
	Decryptor::<Aes128>::new(&key.into(), &iv.into())
		.decrypt_padded_vec_mut::<Pkcs7>(&data)
		.map_err(|_| AidokuError::message("failed to decrypt comicbox image"))
}

fn parse_magic_info(data: &[u8]) -> Result<(i32, i32)> {
	if data.len() < 8 {
		return Err(AidokuError::message("invalid comicbox image data"));
	}
	match data[1] {
		0 => {
			let book_id = data[2] as i32 * 256 + data[3] as i32;
			let page_number = data[4] as i32 * 16_777_216
				+ data[5] as i32 * 65_536
				+ data[6] as i32 * 256
				+ data[7] as i32;
			Ok((book_id, page_number))
		}
		_ => Ok((0, 0)),
	}
}

fn inject_magic_number(data: &mut [u8]) -> Result<()> {
	if data.len() < 12 {
		return Err(AidokuError::message("invalid comicbox image data"));
	}
	match data[0] {
		0 => data[..12].copy_from_slice(&[
			0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
		]),
		1 => data[..8].copy_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		3 => data[..6].copy_from_slice(&[0x47, 0x49, 0x46, 0x38, 0x39, 0x61]),
		4 => data[..12].copy_from_slice(&[
			0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70, 0x61, 0x76, 0x69, 0x66,
		]),
		_ => return Err(AidokuError::message("unknown comicbox image type")),
	}
	Ok(())
}

fn merge_manga_image(image: ImageRef, book_id: i32, page_number: i32) -> ImageRef {
	let width = image.width();
	let height = image.height();
	let count = get_crop_count(book_id, page_number);
	if width <= 0.0 || height <= 0.0 || count <= 1 {
		return image;
	}

	let mut canvas = Canvas::new(width, height);
	let height_i = height as i32;
	let base_height = (height_i / count) as f32;
	let remainder = (height_i % count) as f32;

	for index in 0..count {
		let mut part_height = base_height;
		let mut dst_y = base_height * index as f32;
		let src_y = height - base_height * (index as f32 + 1.0) - remainder;
		let src_y = if index == 0 {
			part_height += remainder;
			src_y
		} else {
			dst_y += remainder;
			src_y
		};

		canvas.copy_image(
			&image,
			Rect::new(0.0, src_y, width, part_height),
			Rect::new(0.0, dst_y, width, part_height),
		);
	}

	canvas.get_image()
}

fn get_crop_count(book_id: i32, page_number: i32) -> i32 {
	let text = format!("{book_id}{page_number}");
	let digest = Md5::digest(text.as_bytes());
	let last = digest[15] & 0x0f;
	match (hex_char(last) as i32) % 10 {
		0 => 44,
		1 => 48,
		2 => 52,
		3 => 56,
		4 => 60,
		5 => 64,
		6 => 68,
		7 => 72,
		8 => 76,
		_ => 80,
	}
}

fn hex_char(value: u8) -> u8 {
	match value {
		0..=9 => b'0' + value,
		_ => b'a' + value - 10,
	}
}

fn unique_trigger_url(url: &str) -> String {
	format!("{PAGE_TRIGGER_URL}?aidoku={}", simple_hash(url))
}

fn simple_hash(value: &str) -> u32 {
	let mut hash = 2_166_136_261u32;
	for byte in value.as_bytes() {
		hash ^= *byte as u32;
		hash = hash.wrapping_mul(16_777_619);
	}
	hash
}

fn is_image_url(url: &str) -> bool {
	let lower = url.to_ascii_lowercase();
	lower.contains(".jpg")
		|| lower.contains(".jpeg")
		|| lower.contains(".png")
		|| lower.contains(".webp")
}

fn strip_site_title(title: String) -> String {
	title.replace(" - 污污漫畫", "").replace(" - 污污漫画", "")
}

fn clean_text(text: String) -> String {
	text.replace('\n', " ")
		.replace('\t', " ")
		.replace("&hearts;", "♥")
		.replace("&hellip;", "...")
		.replace("&amp;", "&")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

fn encode_query(input: &str) -> String {
	let mut output = String::new();
	for byte in input.as_bytes() {
		if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
			output.push(*byte as char);
		} else if *byte == b' ' {
			output.push_str("%20");
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

impl ImageRequestProvider for ComicBox {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("User-Agent", UA)
			.header("Referer", BASE_URL))
	}
}

impl PageImageProcessor for ComicBox {
	fn process_page_image(
		&self,
		response: aidoku::ImageResponse,
		context: Option<PageContext>,
	) -> Result<ImageRef> {
		if let Some(url) = context.and_then(|context| context.get("url").cloned()) {
			recover_image(&url)
		} else {
			Ok(response.image)
		}
	}
}

register_source!(
	ComicBox,
	ListingProvider,
	Home,
	DeepLinkHandler,
	ImageRequestProvider,
	PageImageProcessor
);
