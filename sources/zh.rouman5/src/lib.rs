#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, Home, HomeComponent,
	HomeComponentValue, HomeLayout, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, UpdateStrategy,
	Viewer,
	alloc::{String, Vec, format, string::ToString, vec},
	imports::{html::Element, net::Request},
	prelude::*,
};

const BASE_URL: &str = "https://rouman5.com";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

struct Rouman5;

impl Source for Rouman5 {
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
				let mut url = format!("{BASE_URL}/books?keyword={}", encode_query(query.trim()));
				if page > 1 {
					url = format!("{url}&page={page}");
				}
				url
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
		let body = request_string(&manga_url)?;

		if needs_details {
			manga.title = meta_content(&body, "og:title")
				.or_else(|| title_tag(&body))
				.map(strip_site_title)
				.unwrap_or(manga.title);
			manga.cover = meta_content(&body, "og:image")
				.or_else(|| meta_content(&body, "twitter:image"))
				.or_else(|| first_image_like(&body))
				.or(manga.cover);
			manga.description = meta_name_content(&body, "description")
				.or_else(|| meta_content(&body, "og:description"));
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}

		if needs_chapters {
			manga.chapters = Some(parse_chapters(&body, &manga.key));
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let body = request_string(&absolute_url(&chapter.key))?;
		let mut pages = Vec::new();
		let mut keys = Vec::<String>::new();
		let mut pos = 0;

		while let Some(url) = next_between(&body, &mut pos, "imageUrl\\\":\\\"", "\\\"") {
			let url = unescape_json(&url);
			if !is_image_url(&url) || keys.contains(&url) {
				continue;
			}
			keys.push(url.clone());
			pages.push(Page {
				content: PageContent::Url(url, None),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Rouman5 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl Home for Rouman5 {
	fn get_home(&self) -> Result<HomeLayout> {
		Ok(HomeLayout {
			components: vec![
				manga_list_component("首页", "home")?,
				manga_list_component("全部漫画", "all")?,
			],
		})
	}
}

impl DeepLinkHandler for Rouman5 {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if is_chapter_key(&key) {
			Ok(Some(DeepLinkResult::Chapter {
				manga_key: String::new(),
				key,
			}))
		} else if key.starts_with("/books/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn request_string(url: &str) -> Result<String> {
	Ok(Request::get(url)?
		.header("User-Agent", UA)
		.header("Accept-Encoding", "identity")
		.string()?)
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.header("User-Agent", UA).html()?;
	let entries: Vec<Manga> = html
		.select("a[href^='/books/']")
		.map(|els| {
			let mut keys = Vec::<String>::new();
			els.filter_map(|a| {
				let href = a.attr("href")?;
				let key = normalize_key(&href);
				if keys.contains(&key) || is_chapter_key(&key) {
					return None;
				}

				let title = a
					.select_first(".truncate")
					.and_then(|el| el.text())
					.or_else(|| a.attr("title"))
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
						.select_first("[style*='background-image']")
						.and_then(|el| attr_style_image(&el))
						.or_else(|| {
							a.select_first("img").and_then(|img| {
								attr_url(&img, "src").or_else(|| attr_url(&img, "data-src"))
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
	let has_next_page = !entries.is_empty();

	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn parse_chapters(body: &str, manga_key: &str) -> Vec<Chapter> {
	let mut chapters = Vec::new();
	let mut keys = Vec::<String>::new();
	let mut pos = 0;
	let prefix = if manga_key.starts_with("/books/") {
		manga_key.to_string()
	} else {
		normalize_key(manga_key)
	};

	while let Some(href) = next_between(body, &mut pos, "href=\"/books/", "\"") {
		let key = format!("/books/{href}");
		if !key.starts_with(&format!("{prefix}/")) || keys.contains(&key) || !is_chapter_key(&key) {
			continue;
		}
		keys.push(key.clone());
		let chapter_number = key
			.rsplit('/')
			.next()
			.and_then(|value| value.parse::<i32>().ok())
			.map(|value| (value + 1) as f32);
		let title = chapter_number.map(|value| format!("第{}話", value as i32));
		chapters.push(Chapter {
			key: key.clone(),
			title,
			chapter_number,
			url: Some(absolute_url(&key)),
			..Default::default()
		});
	}

	chapters
}

fn listing_url(id: &str, page: i32) -> String {
	let mut url = match id {
		"all" => format!("{BASE_URL}/books?continued=true"),
		_ => format!("{BASE_URL}/home"),
	};
	if page > 1 {
		let sep = if url.contains('?') { "&" } else { "?" };
		url = format!("{url}{sep}page={page}");
	}
	url
}

fn manga_list_component(title: &str, id: &str) -> Result<HomeComponent> {
	Ok(HomeComponent {
		title: Some(title.into()),
		subtitle: None,
		value: HomeComponentValue::MangaList {
			ranking: false,
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

fn is_chapter_key(key: &str) -> bool {
	let mut parts = key.trim_start_matches('/').split('/');
	matches!(parts.next(), Some("books"))
		&& parts.next().is_some()
		&& parts
			.next()
			.and_then(|value| value.parse::<i32>().ok())
			.is_some()
		&& parts.next().is_none()
}

fn attr_url(el: &Element, name: &str) -> Option<String> {
	el.attr(name).map(|url| absolute_url(&url))
}

fn attr_style_image(el: &Element) -> Option<String> {
	let style = el.attr("style")?.replace("&quot;", "\"");
	between_after(&style, "background-image:url(\"", "\")")
		.or_else(|| between_after(&style, "background-image:url(", ")"))
		.map(|url| absolute_url(&clean_text(url.to_string())))
}

fn meta_content(body: &str, property: &str) -> Option<String> {
	let marker = format!("property=\"{}\" content=\"", property);
	between_after(body, &marker, "\"").map(unescape_html)
}

fn meta_name_content(body: &str, name: &str) -> Option<String> {
	let marker = format!("name=\"{}\" content=\"", name);
	between_after(body, &marker, "\"").map(unescape_html)
}

fn title_tag(body: &str) -> Option<String> {
	between_after(body, "<title>", "</title>").map(unescape_html)
}

fn first_image_like(body: &str) -> Option<String> {
	let mut pos = 0;
	while let Some(url) = next_between(body, &mut pos, "https://r5.", "\"") {
		let url = format!("https://r5.{}", unescape_json(&url));
		if is_image_url(&url) {
			return Some(url);
		}
	}
	None
}

fn is_image_url(url: &str) -> bool {
	let lower = url.to_ascii_lowercase();
	(lower.contains(".jpg")
		|| lower.contains(".jpeg")
		|| lower.contains(".png")
		|| lower.contains(".webp"))
		&& !lower.contains("/loading")
}

fn strip_site_title(title: String) -> String {
	title
		.replace(" | 漫畫免費在線觀看-肉漫屋", "")
		.replace(" - 肉漫屋", "")
		.replace("《", "")
		.replace("》", "")
}

fn between_after<'a>(body: &'a str, start: &str, end: &str) -> Option<&'a str> {
	let idx = body.find(start)? + start.len();
	let len = body[idx..].find(end)?;
	Some(&body[idx..idx + len])
}

fn next_between(body: &str, pos: &mut usize, start: &str, end: &str) -> Option<String> {
	let offset = body[*pos..].find(start)?;
	*pos += offset + start.len();
	let len = body[*pos..].find(end)?;
	let value = body[*pos..*pos + len].to_string();
	*pos += len + end.len();
	Some(value)
}

fn clean_text(text: String) -> String {
	text.replace('\n', " ")
		.replace('\t', " ")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

fn unescape_html(input: &str) -> String {
	input
		.replace("&quot;", "\"")
		.replace("&amp;", "&")
		.replace("&lt;", "<")
		.replace("&gt;", ">")
		.replace("&hellip;", "...")
		.replace("&#x27;", "'")
}

fn unescape_json(input: &str) -> String {
	unescape_html(input)
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

impl ImageRequestProvider for Rouman5 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("User-Agent", UA)
			.header("Referer", BASE_URL)
			.header(
				"Accept",
				"image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
			)
			.header("Accept-Encoding", "identity"))
	}
}

register_source!(
	Rouman5,
	ListingProvider,
	Home,
	DeepLinkHandler,
	ImageRequestProvider
);
