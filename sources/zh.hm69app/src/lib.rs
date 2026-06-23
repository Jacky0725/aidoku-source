#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, Listing, ListingProvider,
	Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString},
	imports::{html::Element, net::Request},
	prelude::*,
};

const BASE_URL: &str = "https://hm.69app.org";

struct Hm69App;

impl Source for Hm69App {
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
				format!("{BASE_URL}/index.php/search?key={}", encode_query(query.trim()))
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
		let html = Request::get(&manga_url)?.html()?;

		if needs_details {
			manga.title = html
				.select_first(".j-comic-title, .comic-title, h1")
				.and_then(|el| el.text())
				.map(clean_text)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first("meta[property='og:image']")
				.and_then(|el| attr_url(&el, "content"))
				.or_else(|| {
					html.select_first(".de-info__cover img, img[data-original]")
						.and_then(|img| attr_url(&img, "data-original").or_else(|| attr_url(&img, "src")))
				});
			manga.description = html
				.select_first(".comic-intro, .intro-total")
				.and_then(|el| el.text())
				.map(clean_text);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}

		if needs_chapters {
			manga.chapters = html.select("a.j-chapter-link[href^='/index.php/chapter/'], a[href^='/index.php/chapter/']").map(|els| {
				let mut keys = Vec::<String>::new();
				els.filter_map(|a| {
					let href = a.attr("href")?;
					let key = normalize_key(&href);
					if keys.contains(&key) {
						return None;
					}
					keys.push(key.clone());
					let title = a.text().map(clean_text).filter(|t| !t.is_empty());
					Some(Chapter {
						key,
						title,
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
			.select("img.lazy-read[data-original], img[data-original]")
			.map(|imgs| {
				imgs.filter_map(|img| {
					let url = attr_url(&img, "data-original")?;
					if !(url.ends_with(".jpg") || url.ends_with(".jpeg") || url.ends_with(".png") || url.ends_with(".webp")) {
						return None;
					}
					Some(Page {
						content: PageContent::Url(url, None),
						..Default::default()
					})
				})
				.collect()
			})
			.unwrap_or_default())
	}
}

impl ListingProvider for Hm69App {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl DeepLinkHandler for Hm69App {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if key.starts_with("/index.php/chapter/") {
			Ok(Some(DeepLinkResult::Chapter {
				manga_key: String::new(),
				key,
			}))
		} else if key.starts_with("/index.php/comic/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries = html
		.select("a[href^='/index.php/comic/']")
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
					.or_else(|| a.text())
					.or_else(|| a.select_first("img").and_then(|img| img.attr("alt")))
					.map(clean_text)?;
				if title.is_empty() || title.contains("更多") {
					return None;
				}
				keys.push(key.clone());
				Some(Manga {
					key,
					title,
					cover: a
						.select_first("img")
						.and_then(|img| attr_url(&img, "data-original").or_else(|| attr_url(&img, "src"))),
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
		"popular" => "/index.php/category/order/hits",
		_ => "/index.php/category/order/addtime",
	};
	if page <= 1 {
		format!("{BASE_URL}{path}")
	} else {
		format!("{BASE_URL}{path}/{page}")
	}
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

register_source!(Hm69App, ListingProvider, DeepLinkHandler);
