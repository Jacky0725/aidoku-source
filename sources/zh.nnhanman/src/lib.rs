#![no_std]

use aidoku::{
	Chapter, ContentRating, FilterValue, Home, HomeComponent, HomeComponentValue, HomeLayout,
	ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page,
	PageContent, PageContext, Result, Source, Viewer,
	alloc::{String, Vec, string::ToString, vec},
	imports::net::Request,
	prelude::*,
};

const BASE_URL: &str = "https://nnhanman.xyz";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

struct NnHanman;

fn request(url: String) -> Result<aidoku::imports::html::Document> {
	Ok(Request::get(url)?
		.header("User-Agent", UA)
		.header("Accept-Encoding", "identity")
		.html()?)
}

fn encode_query(query: String) -> String {
	let mut output = String::new();
	for byte in query.as_bytes() {
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

fn extract_manga_key(href: &str) -> String {
	href.split("/comic/")
		.last()
		.unwrap_or("")
		.split('/')
		.next()
		.unwrap_or("")
		.replace(".html", "")
}

fn absolute_url(path: &str) -> String {
	if path.starts_with("http://") || path.starts_with("https://") {
		path.to_string()
	} else if path.starts_with("//") {
		format!("https:{}", path)
	} else {
		format!("{}{}", BASE_URL, path)
	}
}

fn looks_like_image(url: &str) -> bool {
	if !url.starts_with("http://") && !url.starts_with("https://") {
		return false;
	}

	let lower = url.to_ascii_lowercase();
	if lower.contains("yandex.ru/watch")
		|| lower.contains("/images/logo")
		|| lower.ends_with("/logo.png")
		|| lower.ends_with("favicon.ico")
	{
		return false;
	}

	lower.contains(".jpg")
		|| lower.contains(".jpeg")
		|| lower.contains(".png")
		|| lower.contains(".webp")
		|| lower.contains(".gif")
}

fn parse_manga_cards(html: aidoku::imports::html::Document) -> Vec<Manga> {
	html.select(".itemBox")
		.map(|items| {
			items
				.filter_map(|item| {
					let href = item
						.select_first(".itemImg a")
						.and_then(|a| a.attr("href"))
						.or_else(|| {
							item.select_first(".itemTxt a.title")
								.and_then(|a| a.attr("href"))
						})
						.or_else(|| item.select_first("a.title").and_then(|a| a.attr("href")))
						.or_else(|| item.select_first("a").and_then(|a| a.attr("href")))?;

					if !href.contains("/comic/") {
						return None;
					}

					let key = extract_manga_key(&href);
					if key.is_empty() {
						return None;
					}

					let cover = item
						.select_first(".itemImg img")
						.and_then(|img| {
							img.attr("src")
								.or_else(|| img.attr("data-src"))
								.or_else(|| img.attr("data-original"))
						})
						.or_else(|| item.select_first("img").and_then(|img| img.attr("src")))
						.map(|url| absolute_url(&url));

					let title = item
						.select_first("a.title")
						.and_then(|a| a.attr("title").or_else(|| a.text()))
						.or_else(|| {
							item.select_first(".itemImg a")
								.and_then(|a| a.attr("title"))
						})
						.or_else(|| {
							item.select_first(".itemImg img")
								.and_then(|img| img.attr("alt"))
						})
						.unwrap_or_default()
						.trim()
						.to_string();

					if title.is_empty() {
						return None;
					}

					Some(Manga {
						key: key.clone(),
						title,
						cover,
						url: Some(format!("{}/comic/{}.html", BASE_URL, key)),
						content_rating: ContentRating::NSFW,
						viewer: Viewer::Webtoon,
						..Default::default()
					})
				})
				.collect::<Vec<Manga>>()
		})
		.unwrap_or_default()
}

fn parse_search_manga_cards(html: aidoku::imports::html::Document) -> Vec<Manga> {
	html.select("a.ImgA[href*='/comic/']")
		.map(|items| {
			items
				.filter_map(|item| {
					let href = item.attr("href")?;
					let key = extract_manga_key(&href);
					if key.is_empty() {
						return None;
					}

					let title = item
						.attr("title")
						.or_else(|| item.select_first("img").and_then(|img| img.attr("alt")))
						.or_else(|| item.text())
						.map(|title| title.trim().to_string())
						.filter(|title| !title.is_empty())?;

					let cover = item.select_first("img").and_then(|img| {
						img.attr("src")
							.or_else(|| img.attr("data-src"))
							.or_else(|| img.attr("data-original"))
							.map(|url| absolute_url(&url))
					});

					Some(Manga {
						key: key.clone(),
						title,
						cover,
						url: Some(format!("{}/comic/{}.html", BASE_URL, key)),
						content_rating: ContentRating::NSFW,
						viewer: Viewer::Webtoon,
						..Default::default()
					})
				})
				.collect::<Vec<Manga>>()
		})
		.unwrap_or_default()
}

fn listing_url(id: &str) -> String {
	match id {
		"ranking_weekly" => format!("{}/ranking/weekly", BASE_URL),
		"ranking_monthly" => format!("{}/ranking/monthly", BASE_URL),
		"ranking_all" => format!("{}/ranking/all", BASE_URL),
		_ => format!("{}/update", BASE_URL),
	}
}

fn manga_list_component(title: &str, id: &str, ranking: bool) -> Result<HomeComponent> {
	Ok(HomeComponent {
		title: Some(title.into()),
		subtitle: None,
		value: HomeComponentValue::MangaList {
			ranking,
			page_size: Some(12),
			entries: parse_manga_cards(request(listing_url(id))?)
				.into_iter()
				.map(|manga| manga.into())
				.collect(),
			listing: None,
		},
	})
}

impl Source for NnHanman {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let query = query.filter(|value| !value.trim().is_empty());
		let is_search = query.is_some();
		let url = if let Some(query) = query {
			format!("{}/search/{}/page/{}", BASE_URL, encode_query(query), page)
		} else {
			if page > 1 {
				return Ok(MangaPageResult {
					entries: Vec::new(),
					has_next_page: false,
				});
			}
			format!("{}/update", BASE_URL)
		};

		let entries = if is_search {
			parse_search_manga_cards(request(url)?)
		} else {
			parse_manga_cards(request(url)?)
		};
		let has_next_page = is_search && !entries.is_empty();

		Ok(MangaPageResult {
			entries,
			has_next_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{}/comic/{}.html", BASE_URL, manga.key);
		let html = request(url.clone())?;

		if needs_details {
			manga.title = html
				.select_first(".Introduct_Sub h1")
				.and_then(|node| node.text())
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first(".Introduct_Sub .pic img")
				.and_then(|img| img.attr("src"))
				.map(|url| absolute_url(&url))
				.or(manga.cover);
			manga.authors = html
				.select_first(".Introduct_Sub .sub_r .txtItme")
				.and_then(|node| node.text())
				.map(|author| vec![author.trim().to_string()]);
			manga.tags = html
				.select(".Introduct_Sub .sub_r .txtItme a")
				.map(|nodes| {
					nodes
						.map(|node| node.text().unwrap_or_default())
						.filter(|tag| !tag.trim().is_empty())
						.collect::<Vec<String>>()
				});
			manga.description = html
				.select_first(".txtDesc")
				.and_then(|node| node.text())
				.map(|description| description.trim().to_string());
			manga.url = Some(url);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::Webtoon;
		}

		if needs_chapters {
			let chapter_nodes = html.select("#mh-chapter-list-ol-0 a[href*='/comic/'][href*='/chapter-']");
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(nodes) = chapter_nodes {
				let links = nodes
					.filter(|item| {
						item.attr("href")
							.map(|href| href.contains("/chapter-") && !href.ends_with("chapter-.html"))
							.unwrap_or(false)
					})
					.collect::<Vec<_>>();
				let len = links.len();

				for (index, item) in links.into_iter().enumerate() {
					let Some(href) = item.attr("href") else {
						continue;
					};
					if !href.contains("/comic/") || !href.contains("/chapter-") || href.ends_with("chapter-.html") {
						continue;
					}

					let key = href.trim_start_matches('/').to_string();
					if chapters.iter().any(|chapter| chapter.key == key) {
						continue;
					}

					let title = item
						.attr("title")
						.or_else(|| item.text())
						.map(|title| title.trim().to_string())
						.filter(|title| !title.is_empty());

					chapters.push(Chapter {
						key: key.clone(),
						title,
						chapter_number: Some((len - index) as f32),
						url: Some(absolute_url(&format!("/{}", key))),
						..Default::default()
					});
				}
			}

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let chapter_url = if chapter.key.starts_with("http") {
			chapter.key
		} else if chapter.key.starts_with('/') {
			absolute_url(&chapter.key)
		} else {
			absolute_url(&format!("/{}", chapter.key))
		};

		let html = request(chapter_url)?;

		let pages = html
			.select(".view-imgBox img")
			.map(|nodes| {
				nodes
					.filter_map(|item| {
						let url = item
							.attr("data-original")
							.or_else(|| item.attr("data-src"))
							.or_else(|| item.attr("src"))?;

						if !looks_like_image(&url) {
							return None;
						}

						Some(Page {
							content: PageContent::url(absolute_url(&url)),
							..Default::default()
						})
					})
					.collect::<Vec<Page>>()
			})
			.unwrap_or_default();

		Ok(pages)
	}
}

impl ListingProvider for NnHanman {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		if page > 1 {
			return Ok(MangaPageResult {
				entries: Vec::new(),
				has_next_page: false,
			});
		}

		Ok(MangaPageResult {
			entries: parse_manga_cards(request(listing_url(&listing.id))?),
			has_next_page: false,
		})
	}
}

impl Home for NnHanman {
	fn get_home(&self) -> Result<HomeLayout> {
		Ok(HomeLayout {
			components: vec![
				manga_list_component("最近更新", "latest", false)?,
				manga_list_component("周榜", "ranking_weekly", true)?,
				manga_list_component("月榜", "ranking_monthly", true)?,
				manga_list_component("总榜", "ranking_all", true)?,
			],
		})
	}
}

impl ImageRequestProvider for NnHanman {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("User-Agent", UA)
			.header("Referer", BASE_URL))
	}
}

register_source!(NnHanman, ListingProvider, Home, ImageRequestProvider);
