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

const BASE_URL: &str = "https://www.comicbox.xyz";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

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
				manga_list_component("排行", "rank", true)?,
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
		"rank" => "/rank",
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

register_source!(
	ComicBox,
	ListingProvider,
	Home,
	DeepLinkHandler,
	ImageRequestProvider
);
