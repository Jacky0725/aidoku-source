#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider,
	Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext,
	Result, Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString},
	imports::{html::Element, net::Request},
	prelude::*,
};

const BASE_URL: &str = "https://www.55comics.com";

struct Comics55;

impl Source for Comics55 {
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
					"{BASE_URL}/search.html?keyword={}",
					encode_query(query.trim())
				)
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
				.select_first("h1, .page-title, .video-title a")
				.and_then(|el| el.text())
				.map(clean_text)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first("meta[property='og:image']")
				.and_then(|el| attr_url(&el, "content"))
				.or_else(|| {
					html.select_first("img[data-original], img[data-src]")
						.and_then(|img| {
							attr_url(&img, "data-original").or_else(|| attr_url(&img, "data-src"))
						})
				});
			manga.description = html
				.select_first(".p-t-5.p-b-5, .video-description, .description")
				.and_then(|el| el.text())
				.map(clean_text);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = UpdateStrategy::Always;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url);
		}

		if needs_chapters {
			manga.chapters = html.select("a[href^='/chapter/']").map(|els| {
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
		let chapter_url = absolute_url(&chapter.key);
		let html = Request::get(&chapter_url)?.html()?;
		let mut pages = parse_page_images(&html);
		let mut page_urls = Vec::<String>::new();

		if let Some(links) = html.select(".chapter-left .pagination a[href], .pagination a[href]") {
			for link in links {
				let Some(href) = link.attr("href") else {
					continue;
				};
				let url = absolute_url(&href);
				if url != chapter_url && !page_urls.contains(&url) {
					page_urls.push(url);
				}
			}
		}

		for url in page_urls {
			let html = Request::get(url)?.html()?;
			pages.extend(parse_page_images(&html));
		}

		Ok(pages)
	}
}

impl ListingProvider for Comics55 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl DeepLinkHandler for Comics55 {
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
		} else if key.starts_with("/album/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries = html
		.select("a[href^='/album/']")
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
					.or_else(|| a.select_first("img").and_then(|img| img.attr("title")))
					.map(clean_text)?;
				if title.is_empty() {
					return None;
				}
				keys.push(key.clone());
				Some(Manga {
					key,
					title,
					cover: a.select_first("img").and_then(|img| {
						attr_url(&img, "data-original")
							.or_else(|| attr_url(&img, "data-src"))
							.or_else(|| attr_url(&img, "src"))
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
		"popular" => "/all/order/hits_week",
		_ => "/all/order/update_time",
	};
	if page <= 1 {
		format!("{BASE_URL}{path}.html")
	} else {
		format!("{BASE_URL}{path}/{page}.html")
	}
}

fn parse_page_images(html: &aidoku::imports::html::Document) -> Vec<Page> {
	html.select(".chapter-left img[data-original], .chapter-left img[data-src]")
		.or_else(|| html.select("img[data-original], img[data-src]"))
		.map(|imgs| {
			imgs.filter_map(|img| {
				let url = attr_url(&img, "data-original").or_else(|| attr_url(&img, "data-src"))?;
				if !url.contains("/chapter/") {
					return None;
				}
				Some(Page {
					content: PageContent::Url(url, None),
					..Default::default()
				})
			})
			.collect()
		})
		.unwrap_or_default()
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

impl ImageRequestProvider for Comics55 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("User-Agent", "Mozilla/5.0")
			.header("Referer", BASE_URL))
	}
}

register_source!(
	Comics55,
	ListingProvider,
	DeepLinkHandler,
	ImageRequestProvider
);
