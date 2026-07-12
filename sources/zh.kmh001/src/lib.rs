#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider,
	Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext,
	Result, Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString},
	imports::{html::Element, net::Request},
	prelude::*,
};

const BASE_URL: &str = "https://kmh001.com";

struct Kmh001;

impl Source for Kmh001 {
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
				format!("{BASE_URL}/search?key={}", encode_query(query.trim()))
			}
			_ => listing_url("latest", page),
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
		let body = Request::get(&manga_url)?.string()?;

		if needs_details {
			if let Some(title) = between_after(&body, "\"text-xl font-bold\",\"children\":\"", "\"")
			{
				manga.title = unescape_json(title);
			}
			manga.cover = meta_content(&body, "og:image")
				.or_else(|| meta_content(&body, "twitter:image"))
				.or_else(|| first_image_url(&body))
				.or(manga.cover);
			manga.description = between_after(&body, "\"whitespace-normal\",\"children\":\"", "\"")
				.map(unescape_json);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}

		if needs_chapters {
			manga.chapters = Some(parse_chapters(&body));
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let body = Request::get(absolute_url(&chapter.key))?.string()?;
		let start = body.find("\"images\":[").unwrap_or(0);
		let mut pages = Vec::new();
		let mut pos = start;
		while let Some(url) = next_json_url(&body, &mut pos) {
			if url.ends_with(".jpg")
				|| url.ends_with(".jpeg")
				|| url.ends_with(".png")
				|| url.ends_with(".webp")
			{
				pages.push(Page {
					content: PageContent::Url(url, None),
					..Default::default()
				});
			}
		}
		Ok(pages)
	}
}

impl ListingProvider for Kmh001 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl DeepLinkHandler for Kmh001 {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if key.starts_with("/chapter/") {
			Ok(Some(DeepLinkResult::Chapter {
				manga_key: String::new(),
				key,
			}))
		} else if key.starts_with("/comic/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries = html
		.select("a[href^='/comic/']")
		.map(|els| {
			let mut keys = Vec::<String>::new();
			els.filter_map(|a| {
				let href = a.attr("href")?;
				let key = normalize_key(&href);
				if keys.contains(&key) {
					return None;
				}
				let title = a
					.select_first("h3")
					.and_then(|el| el.text())
					.or_else(|| a.text())
					.map(clean_text)?;
				if title.is_empty() {
					return None;
				}
				keys.push(key.clone());
				let cover = a
					.select_first("img")
					.and_then(|img| attr_url(&img, "src").or_else(|| attr_url(&img, "data-src")))
					.filter(|url| !is_placeholder_image(url))
					.or_else(|| detail_cover_for_key(&key));
				Some(Manga {
					key,
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
	Ok(MangaPageResult {
		entries,
		has_next_page: true,
	})
}

fn parse_chapters(body: &str) -> Vec<Chapter> {
	let mut chapters = Vec::new();
	let mut pos = 0;
	while let Some(offset) = body[pos..].find("\"_id\":\"") {
		pos += offset + 7;
		let Some(end) = body[pos..].find('"') else {
			break;
		};
		let id = &body[pos..pos + end];
		pos += end;
		if id.len() != 24 {
			continue;
		}
		if chapters
			.iter()
			.any(|chapter: &Chapter| chapter.key == format!("/chapter/{id}"))
		{
			continue;
		}
		let window_end = core::cmp::min(body.len(), pos + 700);
		let window = &body[pos..window_end];
		let subtitle = between_after(window, "\"subtitle\":\"", "\"").map(unescape_json);
		let title = between_after(window, "\"title\":\"", "\"").map(unescape_json);
		let chapter_title = match (subtitle, title) {
			(Some(subtitle), Some(title)) if !title.is_empty() => format!("{subtitle} {title}"),
			(Some(subtitle), _) => subtitle,
			(_, Some(title)) => title,
			_ => String::from("章节"),
		};
		let key = format!("/chapter/{id}");
		chapters.push(Chapter {
			key: key.clone(),
			title: Some(chapter_title),
			url: Some(absolute_url(&key)),
			..Default::default()
		});
	}
	chapters
}

fn listing_url(_id: &str, _page: i32) -> String {
	format!("{BASE_URL}/home")
}

fn next_json_url(body: &str, pos: &mut usize) -> Option<String> {
	let marker = "\"url\":\"";
	let offset = body[*pos..].find(marker)?;
	*pos += offset + marker.len();
	let end = body[*pos..].find('"')?;
	let value = unescape_json(&body[*pos..*pos + end]);
	*pos += end;
	Some(value)
}

fn detail_cover_for_key(key: &str) -> Option<String> {
	let body = Request::get(absolute_url(key)).ok()?.string().ok()?;
	meta_content(&body, "og:image")
		.or_else(|| meta_content(&body, "twitter:image"))
		.or_else(|| first_image_url(&body))
}

fn meta_content(body: &str, property: &str) -> Option<String> {
	let marker = format!("{}\" content=\"", property);
	between_after(body, &marker, "\"").map(unescape_json)
}

fn first_image_url(body: &str) -> Option<String> {
	let mut pos = 0;
	while let Some(url) = next_json_url(body, &mut pos) {
		if is_image_url(&url) && !is_placeholder_image(&url) {
			return Some(url);
		}
	}
	None
}

fn is_image_url(url: &str) -> bool {
	let lower = url.to_ascii_lowercase();
	lower.ends_with(".jpg")
		|| lower.ends_with(".jpeg")
		|| lower.ends_with(".png")
		|| lower.ends_with(".webp")
		|| lower.contains(".jpg?")
		|| lower.contains(".jpeg?")
		|| lower.contains(".png?")
		|| lower.contains(".webp?")
}

fn is_placeholder_image(url: &str) -> bool {
	url.contains("/images/loading") || url.contains("favicon") || url.contains("/images/ad/")
}

fn between_after<'a>(body: &'a str, start: &str, end: &str) -> Option<&'a str> {
	let idx = body.find(start)? + start.len();
	let len = body[idx..].find(end)?;
	Some(&body[idx..idx + len])
}

fn normalize_key(url: &str) -> String {
	url.strip_prefix(BASE_URL).unwrap_or(url).to_string()
}

fn absolute_url(url: &str) -> String {
	if url.starts_with("http") {
		url.into()
	} else {
		format!("{BASE_URL}{url}")
	}
}

fn attr_url(el: &Element, name: &str) -> Option<String> {
	el.attr(name).map(|url| {
		if url.starts_with("//") {
			format!("https:{url}")
		} else if url.starts_with('/') {
			format!("{BASE_URL}{url}")
		} else {
			url
		}
	})
}

fn clean_text(text: String) -> String {
	text.replace('\n', " ")
		.replace('\t', " ")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

fn unescape_json(input: &str) -> String {
	input
		.replace("\\u003e", ">")
		.replace("\\u0026", "&")
		.replace("\\/", "/")
		.replace("\\\"", "\"")
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

impl ImageRequestProvider for Kmh001 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("User-Agent", "Mozilla/5.0")
			.header("Referer", BASE_URL))
	}
}

register_source!(
	Kmh001,
	ListingProvider,
	DeepLinkHandler,
	ImageRequestProvider
);
